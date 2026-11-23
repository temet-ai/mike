use sqlx::PgPool;
use sqlx::postgres::PgListener;

use crate::store::persistence::test_support::{
    acquire_ephemeral_db, seed_project, seed_session_turns,
};

const SENTINEL: &str = r#"{"table":"sentinel"}"#;

async fn two_turns(pool: &PgPool) -> anyhow::Result<(i64, i64)> {
    let uuid = uuid::Uuid::new_v4().to_string();
    let project = seed_project(pool, &format!("/judge-queue-notify/{uuid}")).await?;
    sqlx::query(
        "INSERT INTO sessions.sessions (session_uuid, project_id, first_seen, last_seen)
         VALUES ($1::text::uuid, $2, NOW(), NOW())",
    )
    .bind(&uuid)
    .bind(project)
    .execute(pool)
    .await?;
    seed_session_turns(pool, &uuid, project, 2).await?;
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT t.id FROM memory.operator_turns t
           JOIN sessions.sessions s ON s.id = t.session_id
          WHERE s.session_uuid = $1::text::uuid ORDER BY t.id",
    )
    .bind(&uuid)
    .fetch_all(pool)
    .await?;
    let [a, b] = ids[..] else {
        anyhow::bail!("expected two turns, got {ids:?}");
    };
    Ok((a, b))
}

async fn run(pool: &PgPool, sql: &'static str, ids: &[i64]) -> anyhow::Result<()> {
    sqlx::query(sql).bind(ids).execute(pool).await?;
    Ok(())
}

async fn judge_queue_notices_before_sentinel(listener: &mut PgListener) -> anyhow::Result<usize> {
    let mut seen = 0;
    loop {
        let notice = listener.recv().await?;
        if notice.payload() == SENTINEL {
            return Ok(seen);
        }
        let payload: serde_json::Value = serde_json::from_str(notice.payload())?;
        if payload["table"] == "judge_queue" {
            seen += 1;
        }
    }
}

#[tokio::test]
async fn judge_queue_notifies_when_a_turn_is_queued_or_its_failure_is_cleared() -> anyhow::Result<()>
{
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let pool = PgPool::connect(&db.database_url).await?;
    let (a, b) = two_turns(&pool).await?;
    let mut listener = PgListener::connect(&db.database_url).await?;
    listener.listen("proxy.row_changed").await?;

    run(
        &pool,
        "UPDATE memory.operator_turns SET judge_attempt_count = 1 WHERE id = ANY($1)",
        &[a],
    )
    .await?;
    run(
        &pool,
        "UPDATE memory.operator_turns SET judge_failure_reason = 'transient_exhausted' WHERE id = ANY($1)",
        &[b],
    )
    .await?;
    run(
        &pool,
        "UPDATE memory.operator_turns SET judge_queued_at = now() WHERE id = ANY($1)",
        &[a, b],
    )
    .await?;
    run(
        &pool,
        "UPDATE memory.operator_turns SET judge_queued_at = now() WHERE id = ANY($1)",
        &[a],
    )
    .await?;
    run(
        &pool,
        "UPDATE memory.operator_turns SET judge_failure_reason = NULL WHERE id = ANY($1)",
        &[b],
    )
    .await?;
    sqlx::query("SELECT pg_notify('proxy.row_changed', $1)")
        .bind(SENTINEL)
        .execute(&pool)
        .await?;

    assert_eq!(
        judge_queue_notices_before_sentinel(&mut listener).await?,
        2,
        "one notice for the statement that queued both turns, one for the cleared failure"
    );
    pool.close().await;
    db.cleanup().await
}
