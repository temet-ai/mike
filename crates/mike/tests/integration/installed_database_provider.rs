use anyhow::{Context, Result};
use mike::setup::database::validate_existing;
use mike::store::persistence::{PersistError, migrations};
use mike::store::test_support::TestDb;

async fn fixture() -> Result<mike::store::test_support::TestDbHandle> {
    TestDb::bootstrap()
        .await?
        .context("database fixture must run")?
        .acquire_isolated()
        .await
}

#[tokio::test]
async fn installed_provider_validates_current_ledger_without_mutation() -> Result<()> {
    let db = fixture().await?;
    let pool = sqlx::PgPool::connect(&db.database_url).await?;
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM public._sqlx_migrations")
        .fetch_one(&pool)
        .await?;
    let result = validate_existing(&db.database_url).await?;
    assert_eq!(result.server_major, 19);
    assert_eq!(result.pending_migrations, 0);
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM public._sqlx_migrations")
        .fetch_one(&pool)
        .await?;
    assert_eq!(before, after);
    pool.close().await;
    db.cleanup().await
}

#[tokio::test]
async fn installed_provider_rejects_tables_without_ledger() -> Result<()> {
    let db = fixture().await?;
    let pool = sqlx::PgPool::connect(&db.database_url).await?;
    sqlx::query("DROP TABLE public._sqlx_migrations")
        .execute(&pool)
        .await?;
    let result = validate_existing(&db.database_url).await;
    assert!(
        result
            .err()
            .ok_or_else(|| anyhow::anyhow!("missing original ledger"))?
            .to_string()
            .contains("no public._sqlx_migrations ledger")
    );
    let remains_missing: bool =
        sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations') IS NULL")
            .fetch_one(&pool)
            .await?;
    assert!(remains_missing);
    pool.close().await;
    db.cleanup().await
}

#[tokio::test]
async fn installed_provider_rejects_failed_migration_without_repairing_history() -> Result<()> {
    let db = fixture().await?;
    let pool = sqlx::PgPool::connect(&db.database_url).await?;
    sqlx::query("UPDATE public._sqlx_migrations SET success = false WHERE version = (SELECT min(version) FROM public._sqlx_migrations)").execute(&pool).await?;
    let result = validate_existing(&db.database_url).await;
    assert!(
        result
            .err()
            .ok_or_else(|| anyhow::anyhow!("failed migration"))?
            .to_string()
            .contains("failed migration")
    );
    let assessment = migrations::assess(&pool).await;
    assert!(matches!(
        assessment,
        Err(PersistError::SchemaNotCurrent(reason)) if reason.contains("failed migration")
    ));
    assert_doctor_rejects_failed_ledger(&db.database_url).await?;
    let failed: i64 =
        sqlx::query_scalar("SELECT count(*) FROM public._sqlx_migrations WHERE NOT success")
            .fetch_one(&pool)
            .await?;
    assert_eq!(failed, 1);
    pool.close().await;
    db.cleanup().await
}

async fn assert_doctor_rejects_failed_ledger(database_url: &str) -> Result<()> {
    let home = tempfile::tempdir()?;
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_mike"))
        .args(["doctor", "--json"])
        .env_clear()
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_STATE_HOME", home.path().join("state"))
        .env("MIKE_PERSIST_ENABLED", "1")
        .env("MIKE_DATABASE_URL", database_url)
        .env("MIKE_INSPECT_ADDR", "127.0.0.1:1")
        .current_dir(home.path())
        .output()
        .await?;
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let checks = report["checks"]
        .as_array()
        .context("doctor checks missing")?;
    assert!(checks.iter().any(|check| {
        check["id"] == "persistence"
            && check["required"] == true
            && check["status"] == "fail"
            && check["code"] == "DATABASE_SCHEMA_NOT_CURRENT"
    }));
    assert!(
        !checks
            .iter()
            .any(|check| check["code"] == "DATABASE_CURRENT")
    );
    Ok(())
}

#[tokio::test]
async fn installed_provider_rejects_newer_schema_without_changing_ledger() -> Result<()> {
    let db = fixture().await?;
    let pool = sqlx::PgPool::connect(&db.database_url).await?;
    sqlx::query("INSERT INTO public._sqlx_migrations(version, description, success, checksum, execution_time) VALUES (9223372036854775807, 'newer-binary-fixture', true, decode('00','hex'), 0)").execute(&pool).await?;
    let result = validate_existing(&db.database_url).await;
    assert!(
        result
            .err()
            .ok_or_else(|| anyhow::anyhow!("newer ledger"))?
            .to_string()
            .contains("incompatible")
    );
    pool.close().await;
    db.cleanup().await
}
