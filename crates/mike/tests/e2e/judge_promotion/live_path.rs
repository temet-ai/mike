use super::support::{judge_calls, promoted, serve};

async fn outcome(
    pool: &sqlx::PgPool,
    turn: i64,
) -> anyhow::Result<(String, Option<String>, Option<i64>)> {
    Ok(sqlx::query_as(
        "SELECT o.code, t.outcome_source, t.live_verdict_id
           FROM memory.operator_turns t JOIN memory.turn_outcomes o ON o.id = t.outcome_id
          WHERE t.id = $1",
    )
    .bind(turn)
    .fetch_one(pool)
    .await?)
}

#[tokio::test]
async fn live_bundle_set_writes_live_verdict_and_projection() -> anyhow::Result<()> {
    let Some(p) = promoted("12345678-c012-4c12-8c12-c12c12c12c01", 3, true).await? else {
        return Ok(());
    };
    let (promoted_turns, _) = mike::judge_worker::run_live_judge_once(&p.state).await?;
    assert_eq!(promoted_turns, 3);
    for turn in &p.turns {
        let (code, source, verdict) = outcome(&p.pool, *turn).await?;
        assert_eq!(code, "corrected");
        assert_eq!(source.as_deref(), Some("judge"));
        assert!(verdict.is_some());
    }
    let status: String = sqlx::query_scalar("SELECT status FROM eval.bundles WHERE id = $1")
        .bind(p.bundle_id)
        .fetch_one(&p.pool)
        .await?;
    assert_eq!(
        status, "live_ungated",
        "an ungated bundle is marked live_ungated on first sight"
    );
    Ok(())
}

#[tokio::test]
async fn live_bundle_zero_keeps_r3_path_and_writes_no_v2_verdict() -> anyhow::Result<()> {
    let Some(p) = promoted("12345678-c012-4c12-8c12-c12c12c12c02", 2, false).await? else {
        return Ok(());
    };
    let (promoted_turns, rejudged) = mike::judge_worker::run_live_judge_once(&p.state).await?;
    assert_eq!((promoted_turns, rejudged), (0, 0));
    let v2: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memory.turn_verdicts WHERE reaction IS NOT NULL")
            .fetch_one(&p.pool)
            .await?;
    assert_eq!(v2, 0);
    Ok(())
}

#[tokio::test]
async fn machine_text_close_triggers_no_judge_call_and_a_genuine_one_judges() -> anyhow::Result<()>
{
    let Some(p) = promoted("12345678-c012-4c12-8c12-c12c12c12c03", 2, true).await? else {
        return Ok(());
    };
    let machine = mike::judge_worker::run_immediate_once(
        &p.state,
        &p.session_uuid,
        "[SYSTEM NOTIFICATION - NOT USER INPUT] background task finished",
    )
    .await?;
    assert_eq!(machine, 0);
    assert_eq!(judge_calls(&p.or_mock).await, 0);
    let genuine = mike::judge_worker::run_immediate_once(
        &p.state,
        &p.session_uuid,
        "that retry is still untested",
    )
    .await?;
    assert_eq!(genuine, 2);
    assert_eq!(
        judge_calls(&p.or_mock).await,
        2,
        "one call per eligible turn"
    );
    Ok(())
}

#[tokio::test]
async fn operator_turn_dto_and_verdict_history_carry_the_projection() -> anyhow::Result<()> {
    let Some(p) = promoted("12345678-c012-4c12-8c12-c12c12c12c04", 1, true).await? else {
        return Ok(());
    };
    mike::judge_worker::run_live_judge_once(&p.state).await?;
    let turn = p.turns[0];
    let base = serve(&p.state).await?;
    let dto: serde_json::Value = reqwest::get(format!("{base}/api/operator-turns/{turn}"))
        .await?
        .json()
        .await?;
    assert_eq!(dto["reaction"], "corrected");
    assert_eq!(dto["delivery"], "partial");
    assert_eq!(dto["outcome_source"], "judge");
    assert_eq!(dto["legacy"], false);
    assert_eq!(dto["pending"], false);
    let history: serde_json::Value =
        reqwest::get(format!("{base}/api/operator-turns/{turn}/verdicts"))
            .await?
            .json()
            .await?;
    let live_id = history["live_verdict_id"].clone();
    let marked: Vec<&serde_json::Value> = history["verdicts"]
        .as_array()
        .map(|v| {
            v.iter()
                .filter(|r| r["is_live_projection"] == true)
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(marked.len(), 1);
    assert_eq!(marked[0]["id"], live_id);
    Ok(())
}

#[tokio::test]
async fn rejudge_run_rejudges_the_window_and_finishes() -> anyhow::Result<()> {
    let Some(p) = promoted("12345678-c012-4c12-8c12-c12c12c12c05", 2, true).await? else {
        return Ok(());
    };
    sqlx::query("UPDATE eval.bundles SET promoted_at = now() WHERE id = $1")
        .bind(p.bundle_id)
        .execute(&p.pool)
        .await?;
    sqlx::query("INSERT INTO config.settings (key, value) VALUES ('turn_judge.live_window_days', '1'::jsonb) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value")
        .execute(&p.pool)
        .await?;
    p.state.settings_overlay.rcu(|overlay| {
        let mut next = (**overlay).clone();
        next.turn_judge_live_window_days = 1;
        next
    });
    sqlx::query(
        "UPDATE memory.operator_turns SET closed_at = now() - interval '3 days' WHERE id = ANY($1)",
    )
    .bind(&p.turns)
    .execute(&p.pool)
    .await?;
    let run = mike::store::persistence::eval_rejudge::NewRejudgeRun {
        window: mike::store::persistence::eval_rejudge::RejudgeWindow {
            bundle_id: p.bundle_id,
            since: chrono::Utc::now() - chrono::Duration::days(10),
            until: chrono::Utc::now(),
        },
        daily_call_cap: 300,
        estimated_calls: 2,
        estimated_usd: None,
        created_by: "test",
        role: "turn_judge",
    };
    let Some(persist) = p.state.persist() else {
        anyhow::bail!("persistence is off");
    };
    persist.create_rejudge_run(&run).await?;
    let (_, rejudged) = mike::judge_worker::run_live_judge_once(&p.state).await?;
    assert_eq!(rejudged, 2);
    mike::judge_worker::run_live_judge_once(&p.state).await?;
    let status: String =
        sqlx::query_scalar("SELECT status FROM eval.rejudge_runs ORDER BY id DESC LIMIT 1")
            .fetch_one(&p.pool)
            .await?;
    assert_eq!(status, "done");
    Ok(())
}
