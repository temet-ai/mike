use crate::test_db;

use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

const EXPECTED_TABLES: &[(&str, &str)] = &[("messages", "messages"), ("requests", "messages")];

const EXPECTED_INDEXES: &[(&str, &str)] = &[
    ("messages", "messages_pkey"),
    ("requests", "messages_pkey"),
    ("messages", "idx_messages_last_seen"),
    ("requests", "idx_request_messages_message_id"),
];

#[tokio::test]
async fn messages_dedup_tables_and_indexes_exist_after_migration() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping messages-dedup smoke");
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    for (schema, table) in EXPECTED_TABLES {
        let exists: Option<String> = sqlx::query_scalar(
            "SELECT table_name FROM information_schema.tables
             WHERE table_schema = $1 AND table_name = $2",
        )
        .bind(schema)
        .bind(table)
        .fetch_optional(&pool)
        .await?;
        assert!(
            exists.is_some(),
            "expected `{schema}.{table}` table to exist; not found"
        );
    }

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
            "expected index `{schema}.{idx}`; not found in pg_indexes"
        );
    }

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn migration_recorded_in_sqlx_migrations() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping migration-row test");
        return Ok(());
    };
    let handle = test_db.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle.database_url)
        .await?;

    let migration_version: i64 = 20_260_604_000_001;
    let recorded: Option<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE version = $1")
            .bind(migration_version)
            .fetch_optional(&pool)
            .await?;
    assert_eq!(
        recorded,
        Some(migration_version),
        "squash baseline migration {migration_version} must be recorded in _sqlx_migrations"
    );

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
