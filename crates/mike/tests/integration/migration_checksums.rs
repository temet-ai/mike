use mike::store::test_support::TestDb;
use sqlx::postgres::PgPoolOptions;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("src/store/persistence/migrations");

#[tokio::test]
async fn fresh_disposable_database_has_the_complete_matching_migration_ledger() -> anyhow::Result<()>
{
    let factory = TestDb::bootstrap()
        .await?
        .ok_or_else(|| anyhow::anyhow!("required disposable database fixture was not created"))?;
    let database = factory.acquire_isolated().await?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database.database_url)
        .await?;
    let result = verify_ledger(&pool).await;
    pool.close().await;
    database.cleanup().await?;
    result
}

async fn verify_ledger(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    let rows: Vec<(i64, Vec<u8>)> = sqlx::query_as(
        "SELECT version, checksum FROM _sqlx_migrations WHERE success ORDER BY version",
    )
    .fetch_all(pool)
    .await?;
    let stored: std::collections::BTreeMap<i64, Vec<u8>> = rows.into_iter().collect();
    anyhow::ensure!(
        stored.len() == MIGRATOR.iter().count(),
        "the disposable ledger is incomplete or contains extra versions"
    );
    for migration in MIGRATOR.iter() {
        anyhow::ensure!(
            stored.get(&migration.version).map(Vec::as_slice) == Some(migration.checksum.as_ref()),
            "disposable ledger checksum missing or mismatched for migration {}",
            migration.version
        );
    }
    Ok(())
}
