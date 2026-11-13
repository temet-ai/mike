use mike::steerer::guard_test_seams::{SeamPromotion, promote_question, run_demotion};
use mike::store::persistence::dossier_repo::DossierPersist as _;
use mike::store::persistence::steerer_repo::SteererPersist as _;
use serde_json::json;

use crate::turn_facts::{Fixture, NewTurn, Req, fixture, insert_request, insert_turn};

async fn guard_id(f: &Fixture) -> anyhow::Result<i64> {
    Ok(f.handle
        .steerer()
        .guard_steerer()
        .await?
        .map(|g| g.id)
        .unwrap_or_default())
}

async fn turn(f: &Fixture, start: i64) -> anyhow::Result<i64> {
    insert_request(f, &Req::primary(start, 0, "fix the retry")).await?;
    insert_turn(f, &NewTurn::closed(start, start, 0, 30)).await
}

async fn labelled_fire(
    f: &Fixture,
    steerer: i64,
    turn_id: i64,
    mode: &str,
    label: &str,
    arm: Option<&str>,
) -> anyhow::Result<()> {
    let verdict = json!({"quorum": {"relapse": {"votes_fire": 3, "votes_total": 3}}});
    sqlx::query(
        "INSERT INTO steering.evaluations (steerer_id, session_id, operator_turn_id, status, mode, question, trigger_point,
                                           dispatched_at, resolved_at, verdict_json, outcome_label, label_source, labelled_at,
                                           label_evidence, arm, holdback_draw, arm_anchor_turn_id)
         VALUES ($1, $2, $3, 'resolved_correction', $4, 'relapse', 'turn_end', now(), now(), $5, $6, 'next_message', now(),
                 '{}'::jsonb, $7, CASE WHEN $7 IS NULL THEN NULL ELSE 0.01 END, CASE WHEN $7 IS NULL THEN NULL ELSE $3 END)",
    )
    .bind(steerer)
    .bind(f.session_id)
    .bind(turn_id)
    .bind(mode)
    .bind(verdict)
    .bind(label)
    .bind(arm)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

async fn operator_class(f: &Fixture, turn_id: i64) -> anyhow::Result<()> {
    let row = crate::turn_facts::sample_row(turn_id, f.session_id, chrono::Utc::now());
    f.handle.dossier().upsert_turn_facts(&[row]).await?;
    sqlx::query(
        "UPDATE memory.turn_facts SET session_class = 'operator' WHERE operator_turn_id = $1",
    )
    .bind(turn_id)
    .execute(f.handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn gate_with_34_of_40_refuses_wake_then_override_promotes_and_writes_both_rows()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let steerer = guard_id(&f).await?;
    sqlx::query("UPDATE steering.question_policies SET updated_at = now() - interval '1 day' WHERE steerer_id = $1")
        .bind(steerer)
        .execute(f.handle.pool())
        .await?;
    for i in 0..40 {
        let t = turn(&f, 60_000 + i).await?;
        operator_class(&f, t).await?;
        labelled_fire(
            &f,
            steerer,
            t,
            "shadow",
            if i < 34 { "right" } else { "wrong" },
            None,
        )
        .await?;
    }
    let refused = promote_question(&f.handle, steerer, "relapse", "wake", None).await?;
    let SeamPromotion::GateFailed(gate) = refused else {
        anyhow::bail!("expected a failing gate");
    };
    assert_eq!(gate["counts"]["labelled_right"], 34);
    assert_eq!(gate["counts"]["labelled_wrong"], 6);
    assert_eq!(gate["bounds"]["precision"], 0.85);
    assert!(
        gate["bounds"]["low"]
            .as_f64()
            .is_some_and(|l| (l - 0.709).abs() < 0.002),
        "{gate}"
    );
    let promoted = promote_question(
        &f.handle,
        steerer,
        "relapse",
        "wake",
        Some("operator accepts the interval"),
    )
    .await?;
    let SeamPromotion::Promoted(id) = promoted else {
        anyhow::bail!("expected a promotion");
    };
    let (mode, override_reason, change_log): (String, Option<String>, Option<i64>) = sqlx::query_as(
        "SELECT p.mode, pr.override_reason, pr.change_log_id
           FROM steering.question_promotions pr JOIN steering.question_policies p ON p.id = pr.policy_id
          WHERE pr.id = $1",
    )
    .bind(id)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(mode, "wake");
    assert_eq!(
        override_reason.as_deref(),
        Some("operator accepts the interval")
    );
    assert!(
        change_log.is_some(),
        "the JC05 promotion row is written in the same transaction"
    );
    Ok(())
}

#[tokio::test]
async fn precision_below_the_bar_demotes_automatically_with_a_promotion_row() -> anyhow::Result<()>
{
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let steerer = guard_id(&f).await?;
    sqlx::query("UPDATE steering.question_policies SET mode = 'append' WHERE steerer_id = $1 AND question = 'relapse'")
        .bind(steerer)
        .execute(f.handle.pool())
        .await?;
    for i in 0..30 {
        let t = turn(&f, 61_000 + i).await?;
        labelled_fire(
            &f,
            steerer,
            t,
            "live",
            if i < 15 { "right" } else { "wrong" },
            Some("holdback"),
        )
        .await?;
    }
    let demoted = run_demotion(&f.handle).await;
    assert_eq!(demoted, vec!["relapse".to_owned()]);
    let (mode, via): (String, String) = sqlx::query_as(
        "SELECT mode, updated_via FROM steering.question_policies WHERE steerer_id = $1 AND question = 'relapse'",
    )
    .bind(steerer)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!((mode.as_str(), via.as_str()), ("shadow", "auto"));
    let action: String = sqlx::query_scalar(
        "SELECT action FROM steering.question_promotions ORDER BY id DESC LIMIT 1",
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(action, "auto_demote");
    Ok(())
}
