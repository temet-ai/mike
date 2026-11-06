use std::time::Duration;

use anyhow::{Context, Result};
use mike::store::persistence::migrations;
use mike::store::test_support::{TestDb, TestDbHandle};
use serde_json::Value;
use sqlx::PgPool;

#[path = "../src/setup/database/assessment.rs"]
mod database_assessment;

async fn fixture() -> Result<TestDbHandle> {
    TestDb::bootstrap()
        .await?
        .context("database fixture must run")?
        .acquire_isolated()
        .await
}

async fn ledger(pool: &PgPool) -> Result<Vec<Value>> {
    Ok(sqlx::query_scalar(
        "SELECT to_jsonb(ledger) FROM public._sqlx_migrations AS ledger ORDER BY version",
    )
    .fetch_all(pool)
    .await?)
}

async fn doctor(database_url: Option<&str>, persistent: bool) -> Result<Value> {
    let home = tempfile::tempdir()?;
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_mike"));
    command
        .args(["doctor", "--json"])
        .env_clear()
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_STATE_HOME", home.path().join("state"))
        .env("MIKE_PERSIST_ENABLED", if persistent { "1" } else { "0" })
        .env("MIKE_INSPECT_ADDR", "127.0.0.1:1")
        .current_dir(home.path())
        .kill_on_drop(true);
    if let Some(url) = database_url {
        command.env("MIKE_DATABASE_URL", url);
    }
    let output = tokio::time::timeout(Duration::from_secs(20), command.output()).await??;
    let text = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    let report: Value = serde_json::from_str(&text)?;
    assert_eq!(report["read_only"], true);
    for diagnostic in [&text, &stderr] {
        assert!(!diagnostic.contains("postgres://"));
        assert!(!diagnostic.contains("postgresql://"));
        assert!(!diagnostic.contains("private-doctor-secret"));
        assert!(!diagnostic.contains(home.path().to_string_lossy().as_ref()));
        if let Some(url) = database_url {
            assert!(!diagnostic.contains(url));
        }
    }
    assert_eq!(std::fs::read_dir(home.path())?.count(), 0);
    let checks = report["checks"]
        .as_array()
        .context("missing doctor checks")?;
    let failed = checks
        .iter()
        .any(|check| check["required"] == true && check["status"] != "pass");
    assert_eq!(!output.status.success(), failed);
    checks
        .iter()
        .find(|check| check["id"] == "persistence")
        .cloned()
        .context("missing persistence check")
}

fn assert_check(check: &Value, code: &str, status: &str, required: bool) {
    assert_eq!(check["code"], code);
    assert_eq!(check["status"], status);
    assert_eq!(check["required"], required);
}

fn restricted_url(database_url: &str) -> Result<String> {
    let mut url = url::Url::parse(database_url)?;
    let mut pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    let mut options = String::new();
    pairs.retain(|(key, value)| {
        if key == "options" {
            options.push_str(value);
            options.push(' ');
            false
        } else {
            true
        }
    });
    options.push_str("-c role=pg_read_all_data");
    url.set_query(None);
    url.query_pairs_mut()
        .extend_pairs(pairs)
        .append_pair("options", &options);
    Ok(url.into())
}

#[tokio::test]
async fn doctor_current_ledger_requires_database_role_prerequisites() -> Result<()> {
    let db = fixture().await?;
    let pool = PgPool::connect(&db.database_url).await?;
    database_assessment::require_successful_ledger(&pool).await?;
    database_assessment::read(&pool).await?.validate()?;
    database_assessment::extensions(&pool, true, true).await?;
    let before = ledger(&pool).await?;
    assert!(!before.is_empty());
    let healthy = doctor(Some(&db.database_url), true).await?;
    assert_check(&healthy, "DATABASE_CURRENT", "pass", true);
    let url = restricted_url(&db.database_url)?;
    let restricted = PgPool::connect(&url).await?;
    let role: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(&restricted)
        .await?;
    assert_eq!(role, "pg_read_all_data");
    let assessment = migrations::assess(&restricted).await?;
    assert!(assessment.not_current_reason().is_none());
    assert!(!assessment.is_blocking());
    assert_eq!(ledger(&restricted).await?, before);
    let check = doctor(Some(&url), true).await?;
    assert_check(&check, "DATABASE_PREREQUISITES_FAILED", "fail", true);
    assert_eq!(ledger(&pool).await?, before);
    restricted.close().await;
    pool.close().await;
    db.cleanup().await
}

#[tokio::test]
async fn doctor_current_ledger_requires_application_schema() -> Result<()> {
    let db = fixture().await?;
    let pool = PgPool::connect(&db.database_url).await?;
    let before = ledger(&pool).await?;
    sqlx::query("ALTER SCHEMA mike RENAME TO doctor_private_fixture")
        .execute(&pool)
        .await?;
    let check = doctor(Some(&db.database_url), true).await?;
    assert_check(&check, "DATABASE_SCHEMA_NOT_CURRENT", "fail", true);
    assert_eq!(ledger(&pool).await?, before);
    pool.close().await;
    db.cleanup().await
}

#[tokio::test]
async fn doctor_trial_and_unconfigured_database_keep_distinct_readiness() -> Result<()> {
    let trial = doctor(
        Some("postgres://mike:private-doctor-secret@127.0.0.1:1/unused"),
        false,
    )
    .await?;
    assert_check(&trial, "TRIAL_MODE", "pass", false);
    let missing = doctor(None, true).await?;
    assert_check(&missing, "DATABASE_NOT_CONFIGURED", "fail", true);
    let unavailable = doctor(
        Some("postgres://mike:private-doctor-secret@127.0.0.1:1/unused"),
        true,
    )
    .await?;
    assert_check(&unavailable, "DATABASE_TIMEOUT", "fail", true);
    let db = fixture().await?;
    let mut missing_url = url::Url::parse(&db.database_url)?;
    missing_url.set_path("/doctor_missing_database");
    let unavailable = doctor(Some(missing_url.as_str()), true).await?;
    assert_check(&unavailable, "DATABASE_UNAVAILABLE", "fail", true);
    db.cleanup().await
}

#[tokio::test]
async fn doctor_noncurrent_ledgers_keep_schema_code_without_mutation() -> Result<()> {
    for mutation in [
        "DELETE FROM public._sqlx_migrations WHERE version = (SELECT max(version) FROM public._sqlx_migrations)",
        "UPDATE public._sqlx_migrations SET success = false WHERE version = (SELECT min(version) FROM public._sqlx_migrations)",
        "INSERT INTO public._sqlx_migrations(version, description, success, checksum, execution_time) VALUES (9223372036854775807, 'doctor-newer-fixture', true, decode('00','hex'), 0)",
    ] {
        let db = fixture().await?;
        let pool = PgPool::connect(&db.database_url).await?;
        sqlx::query(mutation).execute(&pool).await?;
        let before = ledger(&pool).await?;
        let url = restricted_url(&db.database_url)?;
        let check = doctor(Some(&url), true).await?;
        assert_check(&check, "DATABASE_SCHEMA_NOT_CURRENT", "fail", true);
        assert_eq!(ledger(&pool).await?, before);
        pool.close().await;
        db.cleanup().await?;
    }
    Ok(())
}

#[tokio::test]
async fn doctor_current_ledger_requires_installed_extensions() -> Result<()> {
    let db = fixture().await?;
    let pool = PgPool::connect(&db.database_url).await?;
    let before = ledger(&pool).await?;
    sqlx::query("DROP EXTENSION pg_trgm CASCADE")
        .execute(&pool)
        .await?;
    assert_eq!(ledger(&pool).await?, before);
    let assessment = migrations::assess(&pool).await?;
    assert!(assessment.not_current_reason().is_none());
    assert!(!assessment.is_blocking());
    let error = mike::setup::database::validate_existing(&db.database_url)
        .await
        .err()
        .context("current ledger must require installed extensions")?;
    assert!(error.to_string().contains("pg_trgm"));
    assert!(error.to_string().contains("to be installed"));
    let check = doctor(Some(&db.database_url), true).await?;
    assert_check(&check, "DATABASE_PREREQUISITES_FAILED", "fail", true);
    assert_eq!(ledger(&pool).await?, before);
    pool.close().await;
    db.cleanup().await
}
