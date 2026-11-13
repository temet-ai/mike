use mike::store::persistence::lessons_repo::LessonsPersist as _;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::lessons::NewLessonWrite;
use mike::store::persistence::test_support;

use crate::test_db::TestDb;

type LessonInjectionTestRow = (i64, i64, Option<i64>, String, Option<String>);

#[tokio::test]
async fn record_lesson_injections_lands_rows_with_lane_and_persona() -> anyhow::Result<()> {
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

    let project_id =
        test_support::seed_project(&pool, &format!("w-lesson-injections-{}", Uuid::new_v4()))
            .await?;
    let session_a = Uuid::new_v4().to_string();
    let session_b = Uuid::new_v4().to_string();
    test_support::seed_two_sessions(&pool, &session_a, &session_b, project_id).await?;
    test_support::seed_session_turns(&pool, &session_a, project_id, 1).await?;
    let request_id: i64 = sqlx::query_scalar(
        "SELECT r.id FROM requests.requests r
           JOIN sessions.sessions s ON s.id = r.session_id
          WHERE s.session_uuid = $1::text::uuid",
    )
    .bind(&session_a)
    .fetch_one(&pool)
    .await?;

    let lesson_a = handle
        .lessons()
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "correction",
            text: "always use make targets",
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
            text: "do not touch sweep_at_startup",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;

    handle
        .lessons()
        .record_lesson_injections(
            request_id,
            &session_a,
            "subagent",
            Some("code-reviewer"),
            &[lesson_a.id, lesson_b.id],
        )
        .await?;

    let session_db_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(&session_a)
            .fetch_one(&pool)
            .await?;

    let rows: Vec<LessonInjectionTestRow> = sqlx::query_as(
        "SELECT li.lesson_id, li.request_id, li.session_id, lane.code, li.persona
           FROM memory.lesson_injections li
           JOIN memory.lesson_injection_lanes lane ON lane.id = li.lane_id
          WHERE li.request_id = $1
          ORDER BY li.lesson_id",
    )
    .bind(request_id)
    .fetch_all(&pool)
    .await?;

    assert_eq!(rows.len(), 2, "one row per injected lesson");
    for (lesson_id, req_id, sess_id, lane_code, persona) in &rows {
        assert!([lesson_a.id, lesson_b.id].contains(lesson_id));
        assert_eq!(*req_id, request_id);
        assert_eq!(*sess_id, Some(session_db_id));
        assert_eq!(lane_code, "subagent");
        assert_eq!(persona.as_deref(), Some("code-reviewer"));
    }

    Ok(())
}

#[tokio::test]
async fn record_lesson_injections_empty_ids_is_a_no_op() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    handle
        .lessons()
        .record_lesson_injections(1, &Uuid::new_v4().to_string(), "primary", None, &[])
        .await?;

    Ok(())
}

#[tokio::test]
async fn record_lesson_injections_unknown_lesson_id_returns_err() -> anyhow::Result<()> {
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
        &format!("w-lesson-injections-err-{}", Uuid::new_v4()),
    )
    .await?;
    let session = Uuid::new_v4().to_string();
    let other = Uuid::new_v4().to_string();
    test_support::seed_two_sessions(&pool, &session, &other, project_id).await?;
    test_support::seed_session_turns(&pool, &session, project_id, 1).await?;
    let request_id: i64 = sqlx::query_scalar(
        "SELECT r.id FROM requests.requests r
           JOIN sessions.sessions s ON s.id = r.session_id
          WHERE s.session_uuid = $1::text::uuid",
    )
    .bind(&session)
    .fetch_one(&pool)
    .await?;

    let result = handle
        .lessons()
        .record_lesson_injections(request_id, &session, "primary", None, &[999_999_999])
        .await;
    assert!(
        result.is_err(),
        "a lesson_id with no matching memory.lessons row must FK-violate, not silently insert"
    );

    Ok(())
}
