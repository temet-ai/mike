use super::fixtures::{
    bootstrap_project_session, seed_completed_request, seed_unjudged_operator_turn,
};

use crate::test_db;

use chrono::{DateTime, Duration, Utc};
use mike::store::persistence::{
    NeedsJudgmentQuery, PersistHandle, fetch_sessions_needing_judgment,
};
use test_db::TestDb;
use uuid::Uuid;

const MODEL: &str = "test-judge/window";
const RUBRIC: i32 = 1;

async fn session_with_turn(handle: &PersistHandle) -> anyhow::Result<i64> {
    let pool = handle.pool();
    let (project_id, session_pk, _) =
        bootstrap_project_session(pool, &format!("/test/window/{}", Uuid::new_v4().simple()))
            .await?;
    seed_completed_request(pool, session_pk, project_id).await?;
    seed_unjudged_operator_turn(pool, session_pk, project_id, Utc::now() - Duration::days(3))
        .await?;
    Ok(session_pk)
}

async fn window(
    handle: &PersistHandle,
    session_pk: i64,
) -> anyhow::Result<(Option<DateTime<Utc>>, i32)> {
    Ok(sqlx::query_as(
        "SELECT window_started_at, window_attempts FROM session_judges.attempts
          WHERE session_id = $1 AND judge_model = $2 AND rubric_version = $3",
    )
    .bind(session_pk)
    .bind(MODEL)
    .bind(RUBRIC)
    .fetch_one(handle.pool())
    .await?)
}

async fn selected(handle: &PersistHandle, session_pk: i64, cap: i64) -> anyhow::Result<bool> {
    let query = NeedsJudgmentQuery {
        judge_model: MODEL,
        rubric_version: RUBRIC,
        settled_before: Utc::now() + Duration::hours(1),
        limit: 500,
        max_attempts_per_day: cap,
    };
    let rows = fetch_sessions_needing_judgment(handle.pool(), &query).await?;
    Ok(rows.iter().any(|r| r.session_db_id == session_pk))
}

async fn backdate_failures(handle: &PersistHandle, session_pk: i64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE session_judges.attempts
            SET last_attempt_at = last_attempt_at - interval '2 days'
          WHERE session_id = $1",
    )
    .bind(session_pk)
    .execute(handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn window_counts_attempts_inside_24_hours() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session_pk = session_with_turn(&handle).await?;
    let t0 = Utc::now() - Duration::hours(3);
    for step in 0..3 {
        let at = t0 + Duration::minutes(step * 10);
        handle
            .record_judge_attempt(session_pk, MODEL, RUBRIC, at, true)
            .await?;
    }
    let (started, attempts) = window(&handle, session_pk).await?;
    assert_eq!(attempts, 3);
    assert_eq!(started.map(|t| t.timestamp()), Some(t0.timestamp()));
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn window_restarts_after_24_hours() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session_pk = session_with_turn(&handle).await?;
    let t0 = Utc::now() - Duration::hours(30);
    handle
        .record_judge_attempt(session_pk, MODEL, RUBRIC, t0, true)
        .await?;
    handle
        .record_judge_attempt(session_pk, MODEL, RUBRIC, t0 + Duration::minutes(5), true)
        .await?;
    let later = t0 + Duration::hours(25);
    let restarted = handle
        .record_judge_attempt(session_pk, MODEL, RUBRIC, later, true)
        .await?;
    assert_eq!(restarted, 1, "an attempt 25 hours on restarts the window");
    let (started, _) = window(&handle, session_pk).await?;
    assert_eq!(started.map(|t| t.timestamp()), Some(later.timestamp()));
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn legacy_selection_skips_session_at_daily_cap() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session_pk = session_with_turn(&handle).await?;
    let t0 = Utc::now() - Duration::hours(2);
    for step in 0..3 {
        handle
            .record_judge_attempt(
                session_pk,
                MODEL,
                RUBRIC,
                t0 + Duration::minutes(step),
                true,
            )
            .await?;
    }
    backdate_failures(&handle, session_pk).await?;
    sqlx::query("UPDATE session_judges.attempts SET window_started_at = $2 WHERE session_id = $1")
        .bind(session_pk)
        .bind(t0)
        .execute(handle.pool())
        .await?;
    assert!(
        !selected(&handle, session_pk, 3).await?,
        "three attempts in the window reach a cap of 3"
    );
    assert!(
        selected(&handle, session_pk, 4).await?,
        "the same session is under a cap of 4"
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn legacy_selection_takes_session_again_when_window_rolls() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let session_pk = session_with_turn(&handle).await?;
    let t0 = Utc::now() - Duration::hours(26);
    for step in 0..3 {
        handle
            .record_judge_attempt(
                session_pk,
                MODEL,
                RUBRIC,
                t0 + Duration::minutes(step),
                true,
            )
            .await?;
    }
    backdate_failures(&handle, session_pk).await?;
    sqlx::query("UPDATE session_judges.attempts SET window_started_at = $2 WHERE session_id = $1")
        .bind(session_pk)
        .bind(t0)
        .execute(handle.pool())
        .await?;
    assert!(
        selected(&handle, session_pk, 3).await?,
        "a window older than 24 hours no longer caps the session"
    );
    db.cleanup().await?;
    Ok(())
}
