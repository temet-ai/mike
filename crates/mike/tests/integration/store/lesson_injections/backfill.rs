use sqlx::postgres::PgPoolOptions;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::lesson_injections::BackfillInsertRow;
use mike::store::persistence::lessons::NewLessonWrite;
use mike::store::persistence::test_support;

use crate::test_db::TestDb;

#[tokio::test]
async fn list_lesson_injection_backfill_candidates_finds_marked_requests_and_respects_bounds()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let project_id = test_support::seed_project(&pool, "w-lesson-backfill-candidates").await?;
    let session_a = "00000000-0000-4000-8000-0000000000e1";
    let session_b = "00000000-0000-4000-8000-0000000000e2";
    test_support::seed_two_sessions(&pool, session_a, session_b, project_id).await?;
    test_support::seed_session_turns(&pool, session_a, project_id, 3).await?;

    let session_db_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(session_a)
            .fetch_one(&pool)
            .await?;
    let request_ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM requests.requests WHERE session_id = $1 ORDER BY id")
            .bind(session_db_id)
            .fetch_all(&pool)
            .await?;
    assert_eq!(request_ids.len(), 3, "seed_session_turns must land 3 rows");
    let (envelope_marked, delta_marked, unmarked) =
        (request_ids[0], request_ids[1], request_ids[2]);

    let marker: &[u8] = b"...a proxy-lessons block sits here...";
    sqlx::query(
        "UPDATE requests.requests r
            SET turn_origin_id   = torg.id,
                subagent_persona = 'code-reviewer',
                sent_patch_start = 0,
                sent_patch_end   = 1,
                sent_patch_bytes = $2
           FROM requests.turn_origins torg
          WHERE torg.code = 'subagent' AND r.id = $1",
    )
    .bind(envelope_marked)
    .bind(marker)
    .execute(&pool)
    .await?;

    let message_id: i64 = sqlx::query_scalar(
        "INSERT INTO messages.messages (message_sha256, body, byte_size, role_id)
         SELECT 'w-backfill-candidates-sha', '{}'::jsonb, 2, r.id
           FROM messages.roles r WHERE r.code = 'user'
         RETURNING id",
    )
    .fetch_one(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO requests.deltas
            (request_id, session_id, message_id, change_kind_id, ord,
             sent_patch_start, sent_patch_end, sent_patch_bytes)
         SELECT $1, $2, $3, ck.id, 0, 0, 1, $4
           FROM requests.delta_change_kinds ck
          WHERE ck.code = 'added'",
    )
    .bind(delta_marked)
    .bind(session_db_id)
    .bind(message_id)
    .bind(marker)
    .execute(&pool)
    .await?;
    let _ = unmarked;

    let candidates = handle
        .list_lesson_injection_backfill_candidates(1, None)
        .await?;
    assert_eq!(
        candidates.len(),
        2,
        "only the envelope- and delta-marked requests are candidates"
    );

    let envelope_row = candidates
        .iter()
        .find(|c| c.request_id == envelope_marked)
        .ok_or_else(|| anyhow::anyhow!("envelope-marked request must be a candidate"))?;
    assert_eq!(envelope_row.lane, "subagent");
    assert_eq!(envelope_row.persona.as_deref(), Some("code-reviewer"));
    assert_eq!(envelope_row.session_id, Some(session_db_id));

    let delta_row = candidates
        .iter()
        .find(|c| c.request_id == delta_marked)
        .ok_or_else(|| anyhow::anyhow!("delta-marked request must be a candidate"))?;
    assert_eq!(delta_row.lane, "primary");
    assert_eq!(delta_row.persona, None);

    handle_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_lesson_injection_backfill_candidates_respects_from_to_bounds() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let project_id = test_support::seed_project(&pool, "w-lesson-backfill-bounds").await?;
    let session_a = "00000000-0000-4000-8000-0000000000e9";
    let session_b = "00000000-0000-4000-8000-0000000000ea";
    test_support::seed_two_sessions(&pool, session_a, session_b, project_id).await?;
    test_support::seed_session_turns(&pool, session_a, project_id, 3).await?;

    let session_db_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(session_a)
            .fetch_one(&pool)
            .await?;
    let request_ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM requests.requests WHERE session_id = $1 ORDER BY id")
            .bind(session_db_id)
            .fetch_all(&pool)
            .await?;
    assert_eq!(request_ids.len(), 3, "seed_session_turns must land 3 rows");

    let marker: &[u8] = b"...a proxy-lessons block sits here...";
    for request_id in &request_ids {
        sqlx::query(
            "UPDATE requests.requests
                SET sent_patch_start = 0, sent_patch_end = 1, sent_patch_bytes = $2
              WHERE id = $1",
        )
        .bind(request_id)
        .bind(marker)
        .execute(&pool)
        .await?;
    }

    let unbounded = handle
        .list_lesson_injection_backfill_candidates(1, None)
        .await?;
    assert_eq!(
        unbounded.len(),
        3,
        "no bound must return every marked request"
    );

    let middle_only = handle
        .list_lesson_injection_backfill_candidates(request_ids[1], Some(request_ids[1]))
        .await?;
    assert_eq!(
        middle_only.len(),
        1,
        "from == to == the middle request's id must return only that one"
    );
    assert_eq!(middle_only[0].request_id, request_ids[1]);

    let from_middle = handle
        .list_lesson_injection_backfill_candidates(request_ids[1], None)
        .await?;
    assert_eq!(
        from_middle.len(),
        2,
        "from = middle id, no upper bound, must return the middle and last requests only"
    );

    handle_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn list_lesson_injection_backfill_candidates_does_not_leak_lifted_timeout_to_pool()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    let _ = handle
        .list_lesson_injection_backfill_candidates(1, None)
        .await?;

    let (timeout_after,): (String,) = sqlx::query_as("SHOW statement_timeout")
        .fetch_one(handle.pool())
        .await?;
    assert_eq!(
        timeout_after, "30s",
        "the pool's blanket statement_timeout must survive the candidate scan's SET LOCAL \
         untouched -- a leak here would silently unbound every other pooled query too"
    );

    Ok(())
}

#[tokio::test]
async fn insert_backfilled_lesson_injections_is_idempotent() -> anyhow::Result<()> {
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
        &format!("w-lesson-backfill-insert-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let session = uuid::Uuid::new_v4().to_string();
    let other = uuid::Uuid::new_v4().to_string();
    test_support::seed_two_sessions(&pool, &session, &other, project_id).await?;
    test_support::seed_session_turns(&pool, &session, project_id, 1).await?;

    let session_db_id: i64 =
        sqlx::query_scalar("SELECT id FROM sessions.sessions WHERE session_uuid = $1::text::uuid")
            .bind(&session)
            .fetch_one(&pool)
            .await?;
    let request_id: i64 =
        sqlx::query_scalar("SELECT id FROM requests.requests WHERE session_id = $1")
            .bind(session_db_id)
            .fetch_one(&pool)
            .await?;
    let request_created_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT created_at FROM requests.requests WHERE id = $1")
            .bind(request_id)
            .fetch_one(&pool)
            .await?;

    let lesson = handle
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "correction",
            text: "backfilled historical lesson",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;

    let row = BackfillInsertRow {
        lesson_id: lesson.id,
        request_id,
        session_id: Some(session_db_id),
        injected_at: request_created_at,
        lane: "primary".to_string(),
        persona: None,
    };

    let first_run = handle
        .insert_backfilled_lesson_injections(std::slice::from_ref(&row))
        .await?;
    assert_eq!(first_run, 1, "the first run inserts the reconstructed link");

    let second_run = handle
        .insert_backfilled_lesson_injections(std::slice::from_ref(&row))
        .await?;
    assert_eq!(second_run, 0, "a re-run must conflict, not duplicate");

    let landed: Vec<(i64, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT lesson_id, injected_at FROM memory.lesson_injections WHERE request_id = $1",
    )
    .bind(request_id)
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        landed.len(),
        1,
        "exactly one row must exist after both runs"
    );
    assert_eq!(landed[0].0, lesson.id);
    assert_eq!(
        landed[0].1, request_created_at,
        "injected_at must be the request's own created_at, never now()"
    );

    Ok(())
}
