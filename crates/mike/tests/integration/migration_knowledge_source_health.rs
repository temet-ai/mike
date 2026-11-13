use crate::test_db;

use anyhow::Context;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;
use uuid::Uuid;

const MIGRATION_SQL: &str = include_str!(
    "../../../mike-base/src/store/persistence/migrations/20260922140000_knowledge_source_health.sql"
);

const HEALTH_COLUMNS: [&str; 4] = [
    "last_heartbeat_at",
    "last_failure_at",
    "last_failure",
    "last_failure_generation",
];

async fn connect(url: &str) -> anyhow::Result<PgPool> {
    Ok(PgPoolOptions::new().max_connections(2).connect(url).await?)
}

async fn health_column_count(pool: &PgPool) -> anyhow::Result<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
          WHERE table_schema = 'memory' AND table_name = 'knowledge_sources'
            AND column_name = ANY($1)",
    )
    .bind(&HEALTH_COLUMNS[..])
    .fetch_one(pool)
    .await?;
    Ok(count)
}

async fn seed_source(pool: &PgPool) -> anyhow::Result<(i64, Uuid, Uuid)> {
    let project: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ('/test/health-migration', 'health-migration', now(), now()) RETURNING id",
    )
    .fetch_one(pool)
    .await?;
    let machine = Uuid::new_v4();
    sqlx::query("INSERT INTO projects.machines (machine_uuid, name) VALUES ($1, 'desk')")
        .bind(machine)
        .execute(pool)
        .await?;
    let location: Uuid = sqlx::query_scalar(
        "INSERT INTO projects.locations (project_id, machine_uuid, root_path)
         VALUES ($1, $2, '/work/app') RETURNING location_uuid",
    )
    .bind(project)
    .bind(machine)
    .fetch_one(pool)
    .await?;
    let source: Uuid = sqlx::query_scalar(
        "INSERT INTO memory.knowledge_sources
             (location_uuid, name, root_path, generation, last_scan_at)
         VALUES ($1, 'artifacts', '/work/artifacts', 7, now()) RETURNING source_uuid",
    )
    .bind(location)
    .fetch_one(pool)
    .await?;
    Ok((project, location, source))
}

async fn revert_migration(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::raw_sql(
        "DROP TABLE memory.lesson_file_conflicts;
         ALTER TABLE memory.knowledge_sources
             DROP CONSTRAINT knowledge_sources_failure_report,
             DROP COLUMN last_heartbeat_at,
             DROP COLUMN last_failure_at,
             DROP COLUMN last_failure,
             DROP COLUMN last_failure_generation;",
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn knowledge_source_health_applies_cleanly_to_fresh_empty_db() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping knowledge_source_health fresh-db test");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = connect(&handle.database_url).await?;

    revert_migration(&pool).await?;
    assert_eq!(health_column_count(&pool).await?, 0);
    sqlx::raw_sql(MIGRATION_SQL)
        .execute(&pool)
        .await
        .context("migration SQL must apply cleanly against an empty schema")?;
    assert_eq!(health_column_count(&pool).await?, 4);
    let sources: i64 = sqlx::query_scalar("SELECT count(*) FROM memory.knowledge_sources")
        .fetch_one(&pool)
        .await?;
    assert_eq!(sources, 0);
    sqlx::raw_sql(MIGRATION_SQL)
        .execute(&pool)
        .await
        .context("migration SQL must be safely re-runnable")?;

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn knowledge_source_health_applies_over_seeded_prior_state() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping knowledge_source_health seeded test");
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = connect(&handle.database_url).await?;

    revert_migration(&pool).await?;
    let (project, location, source) = seed_source(&pool).await?;
    let lesson = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO memory.lesson_file_changes (lesson_uuid, project_id, version, payload)
         VALUES ($1, $2, 3, '{}'::jsonb)",
    )
    .bind(lesson)
    .bind(project)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO memory.lesson_file_receipts (location_uuid, lesson_uuid, version)
         VALUES ($1, $2, 3)",
    )
    .bind(location)
    .bind(lesson)
    .execute(&pool)
    .await?;

    sqlx::raw_sql(MIGRATION_SQL)
        .execute(&pool)
        .await
        .context("migration SQL must apply cleanly over existing source rows")?;

    let row: (i64, String, bool, bool, bool, bool) = sqlx::query_as(
        "SELECT generation, root_path, last_scan_at IS NOT NULL,
                last_heartbeat_at IS NULL AND last_failure_at IS NULL,
                last_failure IS NULL, last_failure_generation IS NULL
           FROM memory.knowledge_sources WHERE source_uuid = $1",
    )
    .bind(source)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        row,
        (7, "/work/artifacts".to_string(), true, true, true, true)
    );
    let receipt: i64 = sqlx::query_scalar(
        "SELECT version FROM memory.lesson_file_receipts
          WHERE location_uuid = $1 AND lesson_uuid = $2",
    )
    .bind(location)
    .bind(lesson)
    .fetch_one(&pool)
    .await?;
    assert_eq!(receipt, 3);

    sqlx::query(
        "UPDATE memory.knowledge_sources
            SET last_failure = 'root missing', last_failure_at = now(),
                last_failure_generation = generation
          WHERE source_uuid = $1",
    )
    .bind(source)
    .execute(&pool)
    .await?;
    let half = sqlx::query(
        "UPDATE memory.knowledge_sources SET last_failure_at = NULL WHERE source_uuid = $1",
    )
    .bind(source)
    .execute(&pool)
    .await;
    assert!(
        half.is_err(),
        "a half-written failure must violate the check"
    );

    sqlx::query(
        "INSERT INTO memory.lesson_file_conflicts (location_uuid, lesson_uuid, server_version)
         VALUES ($1, $2, 3)",
    )
    .bind(location)
    .bind(lesson)
    .execute(&pool)
    .await?;
    let bad_fk = sqlx::query(
        "INSERT INTO memory.lesson_file_conflicts (location_uuid, lesson_uuid, server_version)
         VALUES ($1, $2, 1)",
    )
    .bind(location)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await;
    assert!(bad_fk.is_err(), "a conflict must reference a real lesson");

    sqlx::raw_sql(MIGRATION_SQL)
        .execute(&pool)
        .await
        .context("re-running the migration over migrated data must be harmless")?;
    let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM memory.lesson_file_conflicts")
        .fetch_one(&pool)
        .await?;
    assert_eq!(kept, 1);

    pool.close().await;
    handle.cleanup().await?;
    Ok(())
}
