use crate::test_db;

use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;

#[tokio::test]
async fn topic_column_exists() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping topic_column_exists");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let (data_type, is_nullable): (String, String) = sqlx::query_as(
        "SELECT data_type, is_nullable
           FROM information_schema.columns
          WHERE table_schema = 'memory'
            AND table_name = 'lessons'
            AND column_name = 'topic'",
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(data_type, "text", "topic column must be of type text");
    assert_eq!(is_nullable, "YES", "topic column must be nullable");

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn harvester_source_exists() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping harvester_source_exists");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let (id, code): (i64, String) =
        sqlx::query_as("SELECT id, code FROM memory.lesson_sources WHERE code = 'harvester'")
            .fetch_one(&pool)
            .await?;

    assert_eq!(id, 4i64, "harvester source must have id=4");
    assert_eq!(code, "harvester");

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn idx_lessons_topic_exists() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping idx_lessons_topic_exists");
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pg_indexes
          WHERE schemaname = 'memory'
            AND tablename = 'lessons'
            AND indexname = 'idx_lessons_topic'",
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(count, 1, "idx_lessons_topic must exist on memory.lessons");

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn idx_operator_turns_embed_queue_replaces_pending_embed() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping idx_operator_turns_embed_queue_replaces_pending_embed"
        );
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let names: Vec<String> = sqlx::query_scalar(
        "SELECT indexname::text FROM pg_indexes
          WHERE schemaname = 'memory'
            AND tablename = 'operator_turns'
            AND indexname IN ('idx_operator_turns_pending_embed', 'idx_operator_turns_embed_queue')",
    )
    .fetch_all(&pool)
    .await?;

    assert_eq!(
        names,
        vec!["idx_operator_turns_embed_queue".to_string()],
        "the embed queue index serves the drain; the unused pending_embed index is dropped"
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn lesson_with_harvester_source_and_topic_roundtrips() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping lesson_with_harvester_source_and_topic_roundtrips"
        );
        return Ok(());
    };
    let db = factory.acquire_shared().await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db.database_url)
        .await?;

    let project_id: i64 = sqlx::query_scalar(
        "INSERT INTO projects.projects (project_path, display_name, first_seen, last_seen)
         VALUES ($1, $1, NOW(), NOW()) RETURNING id",
    )
    .bind("recall-integration-test")
    .fetch_one(&pool)
    .await?;

    let id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.lessons
             (project_id, kind_id, text, stage_id, source_id, topic)
         SELECT $1, lk.id, $2, ls.id, lsrc.id, $3
         FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
         WHERE lk.code = 'convention'
           AND ls.code = 'raw'
           AND lsrc.code = 'harvester'
         RETURNING id",
    )
    .bind(project_id)
    .bind("prefer sqlx::query! over raw strings")
    .bind("sqlx")
    .fetch_one(&pool)
    .await?;

    let (source_code, topic): (String, Option<String>) = sqlx::query_as(
        "SELECT lsrc.code, l.topic
           FROM memory.lessons l
           JOIN memory.lesson_sources lsrc ON lsrc.id = l.source_id
          WHERE l.id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await?;

    assert_eq!(
        source_code, "harvester",
        "source_id must resolve to 'harvester'"
    );
    assert_eq!(
        topic.as_deref(),
        Some("sqlx"),
        "topic must be stored verbatim"
    );

    db.cleanup().await?;
    Ok(())
}
