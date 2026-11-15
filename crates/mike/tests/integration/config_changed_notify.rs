use crate::test_db;

use std::time::Duration;

use sqlx::postgres::{PgListener, PgPoolOptions};
use test_db::TestDb;
use tokio::time::timeout;

const CHANNEL: &str = "mike.config_changed";

async fn expect_table(listener: &mut PgListener, table: &str) -> anyhow::Result<()> {
    let notification = timeout(Duration::from_secs(2), listener.recv())
        .await
        .map_err(|_| anyhow::anyhow!("no {CHANNEL} notification within 2s for {table}"))??;
    assert_eq!(notification.channel(), CHANNEL);
    assert_eq!(notification.payload(), table);
    Ok(())
}

#[tokio::test]
async fn config_table_writes_notify_config_changed_with_table_name() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping config_changed notify");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;
    let mut listener = PgListener::connect(&handle.database_url).await?;
    listener.listen(CHANNEL).await?;

    sqlx::query(
        "INSERT INTO config.feature_toggles (key, enabled, operator_set) VALUES ('steerer', true, true)
         ON CONFLICT (key) DO UPDATE SET enabled = NOT config.feature_toggles.enabled",
    )
    .execute(&pool)
    .await?;
    expect_table(&mut listener, "feature_toggles").await?;

    sqlx::query(
        "INSERT INTO config.settings (key, value) VALUES ('mailbox_stale_minutes', '42'::jsonb)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .execute(&pool)
    .await?;
    expect_table(&mut listener, "settings").await?;

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
