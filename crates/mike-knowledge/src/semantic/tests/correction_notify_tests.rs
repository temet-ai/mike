use serde::Deserialize;
use sqlx::PgPool;
use sqlx::postgres::PgListener;

use crate::store::models::RowChangedTable;
use crate::store::persistence::test_support::{
    acquire_ephemeral_db, seed_project, seed_session_turns,
};

#[derive(Debug, Deserialize)]
struct RowChanged {
    table: RowChangedTable,
    id: String,
}

async fn turns(pool: &PgPool, count: i64) -> anyhow::Result<(i64, Vec<i64>)> {
    let uuid = uuid::Uuid::new_v4().to_string();
    let project = seed_project(pool, &format!("/correction-notify/{uuid}")).await?;
    let session: i64 = sqlx::query_scalar(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, NOW(), NOW()) RETURNING id",
    )
    .bind(&uuid)
    .bind(project)
    .fetch_one(pool)
    .await?;
    seed_session_turns(pool, &uuid, project, count).await?;
    let ids = sqlx::query_scalar(
        "SELECT id FROM memory.operator_turns WHERE session_id = $1 ORDER BY id",
    )
    .bind(session)
    .fetch_all(pool)
    .await?;
    Ok((session, ids))
}

async fn verdict(
    pool: &PgPool,
    session: i64,
    turn: i64,
    reaction: &str,
    projected: bool,
) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO memory.turn_verdicts
                (operator_turn_id, session_id, mode, rubric_version, input_builder_version, model,
                 reaction, delivery, confidence, rationale, unmet_ask, evidence_quality, projected,
                 as_of, input_text)
         VALUES ($1, $2, 'live', 'test', 'test', 'test', $3, 'none', 0.9, 'r', '', 'absent', $4,
                 NOW(), 'input')
         RETURNING id",
    )
    .bind(turn)
    .bind(session)
    .bind(reaction)
    .bind(projected)
    .fetch_one(pool)
    .await?)
}

async fn labels(pool: &PgPool, turn_ids: &[i64], label: &str) -> anyhow::Result<i64> {
    sqlx::query(
        "INSERT INTO eval.turn_splits (operator_turn_id, split, split_reason)
         SELECT unnest($1::bigint[]), 'dev', 'hash' ON CONFLICT (operator_turn_id) DO NOTHING",
    )
    .bind(turn_ids)
    .execute(pool)
    .await?;
    Ok(sqlx::query_scalar(
        "WITH written AS (
             INSERT INTO eval.labels (subject_kind, subject_id, operator_turn_id, session_id,
                                      session_uuid, axis, label, tier, method, split, source)
             SELECT 'turn', t.id, t.id, t.session_id, s.session_uuid, 'reaction', $2, 'gold',
                    'operator_click', 'dev', 'test'
               FROM memory.operator_turns t JOIN sessions.sessions s ON s.id = t.session_id
              WHERE t.id = ANY($1)
             RETURNING id)
         SELECT max(id) FROM written",
    )
    .bind(turn_ids)
    .bind(label)
    .fetch_one(pool)
    .await?)
}

async fn next_change(listener: &mut PgListener) -> anyhow::Result<RowChanged> {
    let notice = listener.recv().await?;
    Ok(serde_json::from_str(notice.payload())?)
}

#[tokio::test]
async fn corrected_live_verdict_notifies_and_other_verdicts_stay_silent() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let pool = PgPool::connect(&db.database_url).await?;
    let (session, turn_ids) = turns(&pool, 3).await?;
    let [a, b, c] = turn_ids[..] else {
        anyhow::bail!("expected three turns, got {turn_ids:?}");
    };
    let mut listener = PgListener::connect(&db.database_url).await?;
    listener.listen("proxy.row_changed").await?;
    verdict(&pool, session, a, "accepted", true).await?;
    verdict(&pool, session, b, "corrected", false).await?;
    let corrected = verdict(&pool, session, c, "corrected", true).await?;
    let changed = next_change(&mut listener).await?;
    assert_eq!(changed.table, RowChangedTable::CorrectionQualifications);
    assert_eq!(changed.id, format!("memory.turn_verdicts:{corrected}"));
    pool.close().await;
    db.cleanup().await
}

#[tokio::test]
async fn corrected_labels_notify_once_per_statement_and_other_labels_stay_silent()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let pool = PgPool::connect(&db.database_url).await?;
    let (_, turn_ids) = turns(&pool, 3).await?;
    let [a, b, c] = turn_ids[..] else {
        anyhow::bail!("expected three turns, got {turn_ids:?}");
    };
    let mut listener = PgListener::connect(&db.database_url).await?;
    listener.listen("proxy.row_changed").await?;
    labels(&pool, &[a], "accepted").await?;
    let newest = labels(&pool, &[b, c], "corrected").await?;
    let changed = next_change(&mut listener).await?;
    assert_eq!(changed.table, RowChangedTable::CorrectionQualifications);
    assert_eq!(changed.id, format!("eval.labels:{newest}"));
    sqlx::query("SELECT pg_notify('proxy.row_changed', 'marker')")
        .execute(&pool)
        .await?;
    let marker = listener.recv().await?;
    assert_eq!(
        marker.payload(),
        "marker",
        "a two-row label insert sends one notice"
    );
    pool.close().await;
    db.cleanup().await
}
