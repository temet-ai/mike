use sqlx::postgres::PgPoolOptions;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::lessons::NewLessonWrite;
use mike::store::persistence::test_support;

use crate::test_db::TestDb;

#[tokio::test]
async fn lesson_outcomes_excludes_verdicts_judged_before_injection() -> anyhow::Result<()> {
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
        &format!("w-lesson-outcomes-{}", uuid::Uuid::new_v4()),
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

    let lesson = handle
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "correction",
            text: "lesson under outcome-ordering test",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;

    let injected_at = chrono::Utc::now() - chrono::Duration::hours(2);
    sqlx::query(
        "INSERT INTO memory.lesson_injections (lesson_id, request_id, session_id, injected_at, lane_id, persona)
         SELECT $1, $2, $3, $4, lane.id, NULL
           FROM memory.lesson_injection_lanes lane WHERE lane.code = 'primary'",
    )
    .bind(lesson.id)
    .bind(request_id)
    .bind(session_db_id)
    .bind(injected_at)
    .execute(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO session_judges.verdicts
            (session_id, outcome_id, quality_score, judge_model, rubric_version, judged_at)
         SELECT $1, o.id, 0.1, 'test-judge', 1, $2
           FROM session_judges.outcomes o WHERE o.code = 'failure'",
    )
    .bind(session_db_id)
    .bind(injected_at - chrono::Duration::hours(1))
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO session_judges.verdicts
            (session_id, outcome_id, quality_score, judge_model, rubric_version, judged_at)
         SELECT $1, o.id, 0.9, 'test-judge', 1, $2
           FROM session_judges.outcomes o WHERE o.code = 'success'",
    )
    .bind(session_db_id)
    .bind(injected_at + chrono::Duration::hours(1))
    .execute(&pool)
    .await?;

    let rows = handle.lesson_outcomes_observed_after_injection().await?;
    let mine: Vec<_> = rows.iter().filter(|r| r.lesson_id == lesson.id).collect();
    assert_eq!(
        mine.len(),
        1,
        "only the after-injection verdict's outcome bucket must appear, not the before one"
    );
    assert_eq!(mine[0].outcome.as_deref(), Some("success"));
    assert_eq!(mine[0].outcome_count, Some(1));
    assert_eq!(
        mine[0].mean_quality_score,
        Some(0.9),
        "the mean must reflect only the after-injection verdict, not the before one too"
    );
    assert_eq!(mine[0].injection_count, 1);
    assert_eq!(mine[0].distinct_session_count, 1);
    assert_eq!(mine[0].lesson_text, "lesson under outcome-ordering test");

    Ok(())
}

#[tokio::test]
async fn lesson_outcomes_counts_verdicts_once_not_per_injection() -> anyhow::Result<()> {
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
        &format!("w-lesson-outcomes-fanout-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let session = uuid::Uuid::new_v4().to_string();
    let other = uuid::Uuid::new_v4().to_string();
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

    let lesson = handle
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "convention",
            text: "lesson injected many times in one session",
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;

    let injected_at = chrono::Utc::now() - chrono::Duration::hours(3);
    for request_id in &request_ids {
        sqlx::query(
            "INSERT INTO memory.lesson_injections (lesson_id, request_id, session_id, injected_at, lane_id, persona)
             SELECT $1, $2, $3, $4, lane.id, NULL
               FROM memory.lesson_injection_lanes lane WHERE lane.code = 'primary'",
        )
        .bind(lesson.id)
        .bind(request_id)
        .bind(session_db_id)
        .bind(injected_at)
        .execute(&pool)
        .await?;
    }

    sqlx::query(
        "INSERT INTO session_judges.verdicts
            (session_id, outcome_id, quality_score, judge_model, rubric_version, judged_at)
         SELECT $1, o.id, 0.75, 'test-judge', 1, $2
           FROM session_judges.outcomes o WHERE o.code = 'success'",
    )
    .bind(session_db_id)
    .bind(injected_at + chrono::Duration::hours(1))
    .execute(&pool)
    .await?;

    let rows = handle.lesson_outcomes_observed_after_injection().await?;
    let mine: Vec<_> = rows.iter().filter(|r| r.lesson_id == lesson.id).collect();
    assert_eq!(
        mine.len(),
        1,
        "one outcome bucket ('success') must appear for this lesson"
    );
    assert_eq!(
        mine[0].injection_count, 3,
        "3 genuine injections must be counted as 3, not collapsed"
    );
    assert_eq!(
        mine[0].outcome_count,
        Some(1),
        "1 genuine verdict must be counted as 1, NOT 3 (injections × verdicts fan-out)"
    );
    assert_eq!(mine[0].distinct_session_count, 1);
    assert_eq!(mine[0].mean_quality_score, Some(0.75));

    Ok(())
}
