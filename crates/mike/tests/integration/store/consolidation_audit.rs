use chrono::Utc;
use mike::store::persistence::PersistHandle;
use mike::store::persistence::consolidation_audit::ConsolidationActionRecord;
use mike::store::persistence::lessons::NewLessonWrite;
use mike::store::persistence::lessons_repo::LessonsPersist as _;

use crate::test_db::TestDb;

fn skip_record(lesson_id: i64, text: &str, error: &str) -> ConsolidationActionRecord {
    ConsolidationActionRecord {
        lesson_id,
        lesson_text: text.to_string(),
        action_kind: "evaluated",
        outcome: "skipped",
        error: Some(error.to_string()),
        before_stage: Some("raw"),
        after_stage: None,
        decided_by: None,
        distance: None,
        threshold: Some(0.15),
        embedding_model: Some("test-model".to_string()),
        counterpart_lesson_id: None,
        counterpart_uuid: None,
        counterpart_text: None,
        runners_up: serde_json::json!([]),
        rank_detail: None,
    }
}

async fn count_actions_for_lesson(pool: &sqlx::PgPool, lesson_id: i64) -> anyhow::Result<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memory.consolidation_actions WHERE lesson_id = $1",
    )
    .bind(lesson_id)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

#[tokio::test]
async fn delete_consolidation_run_removes_the_row() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let run_id = handle
        .lessons()
        .start_consolidation_run("gc", "schedule")
        .await?;
    handle.lessons().delete_consolidation_run(run_id).await?;

    let still_there: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM memory.consolidation_runs WHERE id = $1)")
            .bind(run_id)
            .fetch_one(&pool)
            .await?;
    assert!(!still_there, "an empty run's row must be gone after delete");

    Ok(())
}

async fn new_lesson(handle: &PersistHandle, project_id: i64, text: &str) -> anyhow::Result<i64> {
    let lesson = handle
        .lessons()
        .create_lesson_with_topic(NewLessonWrite {
            project_id,
            kind: "convention",
            text,
            topic: None,
            source_turn_id: None,
            agent_id: None,
            session_db_id: None,
            request_id: None,
        })
        .await?;
    Ok(lesson.id)
}

async fn run_with(
    handle: &PersistHandle,
    record: &ConsolidationActionRecord,
) -> anyhow::Result<()> {
    let run = handle
        .lessons()
        .start_consolidation_run("session", "manual")
        .await?;
    handle
        .lessons()
        .record_consolidation_actions(run, std::slice::from_ref(record))
        .await?;
    Ok(())
}

#[tokio::test]
async fn record_consolidation_actions_dedups_identical_skips_and_records_changed_ones()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let project_id =
        mike::store::persistence::test_support::seed_project(&pool, "skip-dedup-identical").await?;
    let lesson_id = new_lesson(&handle, project_id, "outage probe lesson").await?;
    let skip = skip_record(
        lesson_id,
        "outage probe lesson",
        "embed failed: connection refused",
    );

    run_with(&handle, &skip).await?;
    run_with(&handle, &skip).await?;

    assert_eq!(
        count_actions_for_lesson(&pool, lesson_id).await?,
        1,
        "an identical consecutive skip must not record a second row"
    );

    let promote = ConsolidationActionRecord {
        lesson_id,
        lesson_text: "outage probe lesson".to_string(),
        action_kind: "promoted_to_daily",
        outcome: "applied",
        error: None,
        before_stage: Some("raw"),
        after_stage: Some("daily"),
        decided_by: Some("embedding_distance"),
        distance: None,
        threshold: Some(0.15),
        embedding_model: Some("test-model".to_string()),
        counterpart_lesson_id: None,
        counterpart_uuid: None,
        counterpart_text: None,
        runners_up: serde_json::json!([]),
        rank_detail: None,
    };
    run_with(&handle, &promote).await?;
    run_with(&handle, &skip).await?;

    assert_eq!(
        count_actions_for_lesson(&pool, lesson_id).await?,
        3,
        "skip, promote, skip must all record — a changed action kind is new information"
    );

    let other_id = new_lesson(&handle, project_id, "different-error lesson").await?;
    run_with(
        &handle,
        &skip_record(
            other_id,
            "different-error lesson",
            "embed failed: connection refused",
        ),
    )
    .await?;
    run_with(
        &handle,
        &skip_record(
            other_id,
            "different-error lesson",
            "embed failed: DNS resolution failed",
        ),
    )
    .await?;

    assert_eq!(
        count_actions_for_lesson(&pool, other_id).await?,
        2,
        "a skip with a DIFFERENT error text must still record — a changed cause is new information"
    );

    Ok(())
}

#[tokio::test]
async fn nightly_ran_since_ignores_failed_runs_but_honors_completed() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;

    let since = Utc::now() - chrono::Duration::minutes(1);

    assert!(
        !handle.lessons().nightly_ran_since(since).await?,
        "no nightly run yet must report false"
    );

    let failed_run = handle
        .lessons()
        .start_consolidation_run("nightly", "schedule")
        .await?;
    handle
        .lessons()
        .finish_consolidation_run(failed_run, "failed", Some("boom"))
        .await?;
    assert!(
        !handle.lessons().nightly_ran_since(since).await?,
        "a failed run must not block a retry"
    );

    let ok_run = handle
        .lessons()
        .start_consolidation_run("nightly", "schedule")
        .await?;
    handle
        .lessons()
        .finish_consolidation_run(ok_run, "completed", None)
        .await?;
    assert!(
        handle.lessons().nightly_ran_since(since).await?,
        "a completed run must satisfy the ran-today gate"
    );

    handle_db.cleanup().await?;
    Ok(())
}
