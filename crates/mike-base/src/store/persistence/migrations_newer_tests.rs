use super::run_patient;
use crate::store::persistence::test_support::TestDb;

#[tokio::test]
async fn run_patient_with_a_migration_from_a_newer_binary_still_starts() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    run_patient(&db.database_url).await?;
    let pool = sqlx::PgPool::connect(&db.database_url).await?;
    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
         VALUES (99991231235959, 'from a newer mike', true, '\\x00'::bytea, 0)",
    )
    .execute(&pool)
    .await?;
    pool.close().await;
    run_patient(&db.database_url).await?;
    db.cleanup().await?;
    Ok(())
}
