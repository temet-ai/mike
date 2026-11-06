use super::fixtures::{assert_no_more, open_listener};

use crate::test_db;

use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

#[tokio::test]
async fn rolled_back_mutation_emits_no_notification() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping session_flow_notifications");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let mut listener = open_listener(&handle.database_url).await?;
    let now = chrono::Utc::now();

    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO memory.consolidation_runs (run_kind_id, trigger_id, status_id, started_at)
         SELECT k.id, t.id, s.id, $1
           FROM memory.consolidation_run_kinds k, memory.consolidation_triggers t, memory.consolidation_run_statuses s
          WHERE k.code = 'session' AND t.code = 'manual' AND s.code = 'running'",
    )
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.rollback().await?;

    assert_no_more(&mut listener).await?;

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
