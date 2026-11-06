use crate::test_db;

use super::fixtures::EXPECTED_INDEXES;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

#[tokio::test]
async fn expected_indexes_exist_after_migration() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping migrations smoke");
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    for (schema, idx) in EXPECTED_INDEXES {
        let exists: Option<String> = sqlx::query_scalar(
            "SELECT indexname FROM pg_indexes
             WHERE schemaname = $1 AND indexname = $2",
        )
        .bind(schema)
        .bind(idx)
        .fetch_optional(&pool)
        .await?;
        assert!(
            exists.is_some(),
            "expected index `{schema}.{idx}` to exist after migrations; not found in pg_indexes"
        );
    }

    let table_exists: Option<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables
         WHERE table_schema = 'requests' AND table_name = 'diagnostics'",
    )
    .fetch_optional(&pool)
    .await?;
    assert!(
        table_exists.is_some(),
        "expected `requests.diagnostics` table to exist after migrations"
    );

    let request_tools_exists: Option<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables
         WHERE table_schema = 'requests' AND table_name = 'tools'",
    )
    .fetch_optional(&pool)
    .await?;
    assert!(
        request_tools_exists.is_some(),
        "expected `requests.tools` table to exist after migrations"
    );

    let pg_trgm_present: Option<String> =
        sqlx::query_scalar("SELECT extname FROM pg_extension WHERE extname = 'pg_trgm'")
            .fetch_optional(&pool)
            .await?;
    assert!(
        pg_trgm_present.is_some(),
        "pg_trgm extension must be installed after Phase A1 migrations"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn requests_session_id_fkey_cascade_constraint_present() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping fkey constraint probe");
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let delete_rule: Option<String> = sqlx::query_scalar(
        "SELECT delete_rule
         FROM information_schema.referential_constraints
         WHERE constraint_schema = 'requests'
           AND constraint_name = 'requests_session_id_fkey'",
    )
    .fetch_optional(&pool)
    .await?;

    assert_eq!(
        delete_rule.as_deref(),
        Some("CASCADE"),
        "requests_session_id_fkey must carry ON DELETE CASCADE; \
         information_schema reported delete_rule = {delete_rule:?}"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
