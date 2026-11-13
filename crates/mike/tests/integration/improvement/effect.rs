use chrono::Utc;
use mike::store::persistence::dossier_repo::DossierPersist as _;
use mike::store::persistence::eval_repo::EvalPersist as _;
use sqlx::PgPool;

use crate::turn_facts::{
    NewTurn, Req, dev_split, insert_request, insert_turn, resolve, sample_row,
};

use super::fixture;

#[tokio::test]
async fn writing_action_effect_rows_uses_a_stable_action_effect_rollup_generation()
-> anyhow::Result<()> {
    let Some(f) = fixture("improvement-effect-rollup").await? else {
        return Ok(());
    };
    let today = Utc::now().date_naive();
    let first_id = f
        .handle
        .eval()
        .current_or_claimed_action_effect_rollup_id(today)
        .await?;
    let second_id = f
        .handle
        .eval()
        .current_or_claimed_action_effect_rollup_id(today)
        .await?;
    assert_eq!(
        first_id, second_id,
        "the same day must reuse one generation"
    );

    let sql_version: String =
        sqlx::query_scalar("SELECT sql_version FROM eval.rollups WHERE id = $1")
            .bind(first_id)
            .fetch_one(f.handle.pool())
            .await?;
    assert_eq!(sql_version, "action-effect-v2");

    let generations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM eval.rollups WHERE for_day = $1 AND sql_version = 'action-effect-v2'",
    )
    .bind(today)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(
        generations, 1,
        "claiming twice on the same day must reuse one eval.rollups row, not create a second"
    );

    f.handle
        .eval()
        .write_action_effect_rows(first_id, Utc::now())
        .await?;
    let rows_after_empty_db: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM eval.rates WHERE rollup_id = $1 AND family = 'action_effect'",
    )
    .bind(first_id)
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(
        rows_after_empty_db, 0,
        "no lesson or guard-question data exists yet, so the writer must write no rows, not fabricate any"
    );
    Ok(())
}

async fn gold_reaction_label(pool: &PgPool, turn_id: i64, label: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO eval.labels (subject_kind, subject_id, operator_turn_id, session_id, session_uuid,
                                  axis, label, tier, method, split, source)
         SELECT 'turn', t.id, t.id, t.session_id, s.session_uuid, 'reaction', $2, 'gold', 'operator_click', 'dev', 'test'
           FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
          WHERE t.id = $1",
    )
    .bind(turn_id)
    .bind(label)
    .execute(pool)
    .await?;
    Ok(())
}

async fn real_replied_turn(
    tf: &crate::turn_facts::Fixture,
    request_id: i64,
    at_secs: i64,
    label: &str,
) -> anyhow::Result<i64> {
    insert_request(
        tf,
        &Req::primary(request_id, at_secs, "does this look right?"),
    )
    .await?;
    let turn = insert_turn(
        tf,
        &NewTurn::closed(request_id, request_id, at_secs, at_secs + 5),
    )
    .await?;
    let mut row = sample_row(turn, tf.session_id, Utc::now());
    resolve(&mut row);
    row.text_chars = 20;
    tf.handle.dossier().upsert_turn_facts(&[row]).await?;
    dev_split(tf, turn).await?;
    gold_reaction_label(tf.handle.pool(), turn, label).await?;
    Ok(turn)
}

#[tokio::test]
async fn writing_action_effect_rows_writes_a_real_before_and_after_share_for_a_promoted_lesson()
-> anyhow::Result<()> {
    let Some(tf) = crate::turn_facts::fixture().await? else {
        return Ok(());
    };

    let before_turn = real_replied_turn(&tf, 95_010, 0, "corrected").await?;

    let lesson_anchor = tf.base + chrono::Duration::seconds(30);
    let lesson_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id, created_at)
         SELECT $1, lk.id, 'run the tests before committing', ls.id, lsrc.id, $2
           FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
          WHERE lk.code = 'correction' AND ls.code = 'daily' AND lsrc.code = 'judge'
         RETURNING id",
    )
    .bind(tf.project_id)
    .bind(lesson_anchor)
    .fetch_one(tf.handle.pool())
    .await?;

    let after_turn = real_replied_turn(&tf, 95_020, 60, "accepted").await?;

    let rollup_id = tf
        .handle
        .eval()
        .current_or_claimed_action_effect_rollup_id(Utc::now().date_naive())
        .await?;
    let now = crate::seed::database_now(tf.handle.pool()).await?;
    let written = tf
        .handle
        .eval()
        .write_action_effect_rows(rollup_id, now)
        .await?;
    assert!(
        written >= 2,
        "expected at least a before and an after row, wrote {written}"
    );

    tf.handle
        .eval()
        .write_action_effect_rows(rollup_id, now)
        .await?;
    assert_eq!(
        tf.handle
            .eval()
            .latest_completed_action_effect_day()
            .await?,
        Some(Utc::now().date_naive())
    );
    let rows: Vec<(String, i32, i32)> = sqlx::query_as(
        "SELECT population, k, n FROM eval.rates
          WHERE rollup_id = $1 AND family = 'action_effect' AND dimension = 'lesson'
            AND dimension_value = $2
          ORDER BY population",
    )
    .bind(rollup_id)
    .bind(lesson_id.to_string())
    .fetch_all(tf.handle.pool())
    .await?;
    assert_eq!(
        rows.len(),
        2,
        "expected exactly one before row and one after row for this lesson, got {rows:?}"
    );
    let before = rows
        .iter()
        .find(|(p, ..)| p == "before")
        .ok_or_else(|| anyhow::anyhow!("before row"))?;
    let after = rows
        .iter()
        .find(|(p, ..)| p == "after")
        .ok_or_else(|| anyhow::anyhow!("after row"))?;
    assert_eq!(
        (before.1, before.2),
        (1, 1),
        "the turn before the lesson was corrected: k=1 of n=1"
    );
    assert_eq!(
        (after.1, after.2),
        (0, 1),
        "the turn after the lesson was accepted, not corrected: k=0 of n=1"
    );
    let _ = (before_turn, after_turn);
    Ok(())
}

#[tokio::test]
async fn operator_raw_lesson_has_computed_zero_rows_and_claim_is_not_completion()
-> anyhow::Result<()> {
    let Some(tf) = crate::turn_facts::fixture().await? else {
        return Ok(());
    };
    let lesson_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id)
         SELECT $1, lk.id, 'trust the command exit code', ls.id, src.id
         FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources src
         WHERE lk.code='correction' AND ls.code='raw' AND src.code='operator' RETURNING id",
    )
    .bind(tf.project_id)
    .fetch_one(tf.handle.pool())
    .await?;
    let rollup = tf
        .handle
        .eval()
        .current_or_claimed_action_effect_rollup_id(Utc::now().date_naive())
        .await?;
    assert_eq!(
        tf.handle
            .eval()
            .latest_completed_action_effect_day()
            .await?,
        None
    );
    tf.handle
        .eval()
        .write_action_effect_rows(rollup, Utc::now())
        .await?;
    let row = tf
        .handle
        .eval()
        .get_lesson_action(lesson_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("lesson missing"))?;
    assert_eq!((row.n_before, row.n_after), (Some(0), Some(0)));
    assert!(
        tf.handle
            .eval()
            .list_lesson_actions(None)
            .await?
            .iter()
            .any(|row| row.lesson_id == lesson_id)
    );
    Ok(())
}

#[tokio::test]
async fn lesson_exposure_counts_exact_turn_once_and_keeps_missing_usage_unknown()
-> anyhow::Result<()> {
    let Some(tf) = crate::turn_facts::fixture().await? else {
        return Ok(());
    };
    let turn = real_replied_turn(&tf, 95_100, 0, "accepted").await?;
    crate::turn_facts::attach(&tf, turn, &[95_100]).await?;
    sqlx::query(
        "UPDATE memory.operator_turns SET reaction='accepted', delivery='delivered' WHERE id=$1",
    )
    .bind(turn)
    .execute(tf.handle.pool())
    .await?;
    let lesson: i64 = sqlx::query_scalar(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id)
         SELECT $1, lk.id, 'verify outputs', ls.id, src.id
         FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources src
         WHERE lk.code='correction' AND ls.code='daily' AND src.code='operator' RETURNING id",
    )
    .bind(tf.project_id)
    .fetch_one(tf.handle.pool())
    .await?;
    sqlx::query(
        "INSERT INTO memory.lesson_injections (lesson_id,request_id,session_id,lane_id,injected_at)
        SELECT $1,95100,$2,id,$3 FROM memory.lesson_injection_lanes WHERE code='primary'",
    )
    .bind(lesson)
    .bind(tf.session_id)
    .bind(tf.base)
    .execute(tf.handle.pool())
    .await?;
    real_replied_turn(&tf, 95_110, 60, "corrected").await?;
    let rows = tf
        .handle
        .eval()
        .list_lesson_exposures(
            None,
            tf.base - chrono::Duration::seconds(1),
            tf.base + chrono::Duration::hours(1),
        )
        .await?;
    let row = rows
        .iter()
        .find(|r| r.lesson_id == lesson)
        .ok_or_else(|| anyhow::anyhow!("missing exposure"))?;
    assert_eq!(
        (
            row.recorded_injections,
            row.exposed_turns,
            row.accepted_turns,
            row.corrected_turns
        ),
        (1, 1, 1, 0)
    );
    assert_eq!(
        (row.input_tokens, row.output_tokens, row.usage_requests),
        (None, None, 0)
    );
    assert_eq!(row.requests, 1);
    assert_eq!(row.unattributed_injections, 0);
    sqlx::query("UPDATE requests.requests SET claude_code_agent_id='child' WHERE id=95100")
        .execute(tf.handle.pool())
        .await?;
    let excluded = tf
        .handle
        .eval()
        .list_lesson_exposures(
            None,
            tf.base - chrono::Duration::seconds(1),
            tf.base + chrono::Duration::hours(1),
        )
        .await?;
    let row = excluded
        .iter()
        .find(|r| r.lesson_id == lesson)
        .ok_or_else(|| anyhow::anyhow!("missing excluded exposure"))?;
    assert_eq!(
        (
            row.recorded_injections,
            row.unattributed_injections,
            row.exposed_turns,
            row.requests
        ),
        (1, 1, 0, 0)
    );
    Ok(())
}
