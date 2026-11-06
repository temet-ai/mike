use sqlx::postgres::PgPoolOptions;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::lessons::NewLessonWrite;
use mike::store::persistence::test_support;

use crate::store::support::seed_agent;
use crate::test_db::TestDb;

#[tokio::test]
async fn create_lesson_resolves_agent_and_session_fks() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let project_id = test_support::seed_project(
        &pool,
        &format!("w3-create-resolve-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let session = uuid::Uuid::new_v4().to_string();
    let other = uuid::Uuid::new_v4().to_string();
    test_support::seed_two_sessions(&pool, &session, &other, project_id).await?;
    test_support::seed_session_turns(&pool, &session, project_id, 1).await?;
    let (request_id, session_db_id): (i64, i64) = sqlx::query_as(
        "SELECT r.id, r.session_id FROM requests.requests r
           JOIN sessions.sessions s ON s.id = r.session_id
          WHERE s.session_uuid = $1::text::uuid LIMIT 1",
    )
    .bind(&session)
    .fetch_one(&pool)
    .await?;
    let (agent_db_id, _agent_uuid) =
        seed_agent(&pool, &format!("w3-agent-{}", uuid::Uuid::new_v4())).await?;

    let inserted = handle
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "correction",
            text: "  Mind The Trailing Space  ",
            topic: None,
            source_turn_id: None,
            agent_id: Some(agent_db_id),
            session_db_id: Some(session_db_id),
            request_id: Some(request_id),
        })
        .await?;

    let row: (Option<i64>, Option<i64>, Option<i64>, Option<Vec<u8>>) = sqlx::query_as(
        "SELECT agent_id, session_id, request_id, text_sha256
           FROM memory.lessons WHERE uuid = $1",
    )
    .bind(inserted.uuid)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        row.0,
        Some(agent_db_id),
        "agent uuid must resolve to its bigint id"
    );
    assert_eq!(
        row.1,
        Some(session_db_id),
        "session string must resolve to its bigint sessions.id"
    );
    assert_eq!(
        row.2,
        Some(request_id),
        "request_id bigint must be stored verbatim"
    );
    let sha = row
        .3
        .ok_or_else(|| anyhow::anyhow!("text_sha256 must be set"))?;
    assert_eq!(
        sha.len(),
        32,
        "text_sha256 must be a 32-byte SHA-256 digest"
    );

    Ok(())
}

#[tokio::test]
async fn create_harvested_lesson_sets_source_and_topic() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let project_id = test_support::seed_project(
        &pool,
        &format!("p13-harvest-source-{}", uuid::Uuid::new_v4()),
    )
    .await?;

    let inserted = handle
        .create_harvested_lesson(NewLessonWrite {
            project_id,
            kind: "correction",
            text: "use PersistHandle for all DB access",
            topic: Some("architecture"),
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;

    let (source_code, topic): (String, Option<String>) = sqlx::query_as(
        "SELECT lsrc.code, l.topic
           FROM memory.lessons l
           JOIN memory.lesson_sources lsrc ON lsrc.id = l.source_id
          WHERE l.uuid = $1",
    )
    .bind(
        inserted
            .ok_or_else(|| anyhow::anyhow!("fresh text must insert, not dedup"))?
            .uuid,
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(
        source_code, "harvester",
        "create_harvested_lesson must set source_id to the 'harvester' row"
    );
    assert_eq!(
        topic.as_deref(),
        Some("architecture"),
        "topic must be stored verbatim"
    );

    Ok(())
}

#[tokio::test]
async fn delete_lesson_archives_and_never_destroys_the_row() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let project_id = test_support::seed_project(
        &pool,
        &format!("delete-lesson-archives-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let lesson_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id)
             SELECT $1, lk.id, $2, ls.id, lsrc.id
               FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
              WHERE lk.code = 'convention' AND ls.code = 'daily' AND lsrc.code = 'operator'
             RETURNING id",
    )
    .bind(project_id)
    .bind("delete_lesson must archive, never destroy, this row")
    .fetch_one(&pool)
    .await?;

    let deleted = handle.delete_lesson(lesson_id).await?;
    assert!(deleted, "delete_lesson must report the live row as retired");

    let row_count: i64 = sqlx::query_scalar("SELECT count(*) FROM memory.lessons WHERE id = $1")
        .bind(lesson_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        row_count, 1,
        "the row must still be present after delete_lesson"
    );

    let (stage_code, reason_code, archived_at): (
        String,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT ls.code, lar.code, l.archived_at
               FROM memory.lessons l
               JOIN memory.lesson_stages ls ON ls.id = l.stage_id
               LEFT JOIN memory.lesson_archived_reasons lar ON lar.id = l.archived_reason_id
              WHERE l.id = $1",
    )
    .bind(lesson_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        stage_code, "archived",
        "delete_lesson must archive, not delete"
    );
    assert_eq!(
        reason_code.as_deref(),
        Some("operator"),
        "the archive reason must record an operator-initiated delete"
    );
    assert!(archived_at.is_some(), "archived_at must be stamped");

    Ok(())
}
