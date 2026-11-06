use super::{fixture, starting, stopping};

#[tokio::test]
async fn stopping_migration_preserves_existing_rows_and_accepts_new_state() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let before = starting(&f.handle, None).await?;
    sqlx::raw_sql(
        "ALTER TABLE launch.fleets DROP CONSTRAINT fleets_state_check;
         ALTER TABLE launch.fleets ADD CONSTRAINT fleets_state_check
         CHECK (state IN ('draft', 'ready', 'starting', 'running', 'failed', 'done'));",
    )
    .execute(f.handle.pool())
    .await?;
    sqlx::raw_sql(include_str!(
        "../../../../src/store/persistence/migrations/20261014070100_fleet_stopping.sql"
    ))
    .execute(f.handle.pool())
    .await?;
    assert_eq!(
        f.handle.find_fleet(&before.fleet_uuid.to_string()).await?,
        Some(before.clone())
    );
    assert_eq!(stopping(&f.handle, &before).await?.state, "stopping");
    let invalid = sqlx::query("UPDATE launch.fleets SET state = 'paused' WHERE fleet_uuid = $1")
        .bind(before.fleet_uuid)
        .execute(f.handle.pool())
        .await;
    assert!(invalid.is_err());
    f.db.cleanup().await
}
