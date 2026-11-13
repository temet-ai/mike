use mike::store::persistence::lessons_repo::LessonsPersist as _;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::lessons::NewLessonWrite;
use mike::store::persistence::test_support;

use crate::test_db::TestDb;

#[tokio::test]
async fn session_lessons_counts_distinct_lesson_not_injections() -> anyhow::Result<()> {
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
        &format!("w-session-lessons-count-{}", Uuid::new_v4()),
    )
    .await?;
    let session = Uuid::new_v4().to_string();
    let other = Uuid::new_v4().to_string();
    test_support::seed_two_sessions(&pool, &session, &other, project_id).await?;
    test_support::seed_session_turns(&pool, &session, project_id, 3).await?;

    let session_db_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(&session)
            .fetch_one(&pool)
            .await?;
    let request_ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM requests.requests WHERE session_id = $1 ORDER BY id")
            .bind(session_db_id)
            .fetch_all(&pool)
            .await?;
    assert_eq!(request_ids.len(), 3, "seed_session_turns must land 3 rows");

    let injected_lesson = handle
        .lessons()
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "convention",
            text: "lesson injected 3 times into one session",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    for request_id in &request_ids {
        handle
            .lessons()
            .record_lesson_injections(
                *request_id,
                &session,
                "primary",
                None,
                &[injected_lesson.id],
            )
            .await?;
    }

    let recorded_lesson = handle
        .lessons()
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "correction",
            text: "lesson recorded with this session as provenance",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: Some(session_db_id),
            request_id: None,
        })
        .await?;

    let uuid = Uuid::parse_str(&session)?;
    let Some(result) = handle.lessons().session_lessons(uuid).await? else {
        anyhow::bail!("a session that exists must return Some, not None");
    };

    assert_eq!(
        result.injected.len(),
        1,
        "3 injections of the same lesson must collapse to 1 distinct lesson"
    );
    assert_eq!(result.injected[0].lesson_id, injected_lesson.id);
    assert_eq!(
        result.injected[0].injection_count, 3,
        "3 genuine injections must be counted as 3, not collapsed"
    );
    assert_eq!(
        result.injected[0].lesson_text,
        "lesson injected 3 times into one session"
    );
    assert_eq!(result.injected[0].kind, "convention");

    assert_eq!(result.recorded.len(), 1);
    assert_eq!(result.recorded[0].lesson_id, recorded_lesson.id);
    assert_eq!(
        result.recorded[0].lesson_text,
        "lesson recorded with this session as provenance"
    );
    assert_eq!(result.recorded[0].kind, "correction");

    Ok(())
}

#[tokio::test]
async fn session_lessons_excludes_injections_from_other_sessions() -> anyhow::Result<()> {
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
        &format!("w-session-lessons-exclude-{}", Uuid::new_v4()),
    )
    .await?;
    let session_a = Uuid::new_v4().to_string();
    let session_b = Uuid::new_v4().to_string();
    test_support::seed_two_sessions(&pool, &session_a, &session_b, project_id).await?;
    test_support::seed_session_turns(&pool, &session_a, project_id, 1).await?;
    test_support::seed_session_turns(&pool, &session_b, project_id, 1).await?;

    let request_id_a: i64 = sqlx::query_scalar(
        "SELECT r.id FROM requests.requests r
           JOIN sessions.sessions s ON s.id = r.session_id
          WHERE s.session_uuid = $1::text::uuid",
    )
    .bind(&session_a)
    .fetch_one(&pool)
    .await?;
    let request_id_b: i64 = sqlx::query_scalar(
        "SELECT r.id FROM requests.requests r
           JOIN sessions.sessions s ON s.id = r.session_id
          WHERE s.session_uuid = $1::text::uuid",
    )
    .bind(&session_b)
    .fetch_one(&pool)
    .await?;

    let lesson_a = handle
        .lessons()
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "warning",
            text: "lesson for session A",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    let lesson_b = handle
        .lessons()
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "warning",
            text: "lesson for session B",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    handle
        .lessons()
        .record_lesson_injections(request_id_a, &session_a, "primary", None, &[lesson_a.id])
        .await?;
    handle
        .lessons()
        .record_lesson_injections(request_id_b, &session_b, "primary", None, &[lesson_b.id])
        .await?;

    let uuid_a = Uuid::parse_str(&session_a)?;
    let Some(result) = handle.lessons().session_lessons(uuid_a).await? else {
        anyhow::bail!("session A exists and must return Some");
    };

    assert_eq!(
        result.injected.len(),
        1,
        "only session A's own lesson must appear"
    );
    assert_eq!(result.injected[0].lesson_id, lesson_a.id);
    assert!(
        result.injected.iter().all(|r| r.lesson_id != lesson_b.id),
        "session B's lesson must not leak into session A's list"
    );

    Ok(())
}

#[tokio::test]
async fn session_lessons_empty_session_returns_empty_lists_and_unknown_session_returns_none()
-> anyhow::Result<()> {
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
        &format!("w-session-lessons-empty-{}", Uuid::new_v4()),
    )
    .await?;
    let session = Uuid::new_v4().to_string();
    let other = Uuid::new_v4().to_string();
    test_support::seed_two_sessions(&pool, &session, &other, project_id).await?;

    let uuid = Uuid::parse_str(&session)?;
    let Some(result) = handle.lessons().session_lessons(uuid).await? else {
        anyhow::bail!("a real session with no lesson activity must still return Some");
    };
    assert!(result.injected.is_empty());
    assert!(result.recorded.is_empty());

    let unknown = Uuid::new_v4();
    let unknown_result = handle.lessons().session_lessons(unknown).await?;
    assert!(
        unknown_result.is_none(),
        "a session UUID that matches no row must return None"
    );

    Ok(())
}
