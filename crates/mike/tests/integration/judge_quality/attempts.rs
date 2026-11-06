use super::fixtures::{
    bootstrap_project_session, resolve_outcome_id, seed_completed_request,
    seed_unjudged_operator_turn,
};

use crate::test_db;

use chrono::{DateTime, Duration, Utc};
use mike::store::persistence::{
    NeedsJudgmentQuery, SessionForJudgment, fetch_sessions_needing_judgment,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use test_db::TestDb;
use uuid::Uuid;

const MODEL: &str = "test-judge/attempts";
const RUBRIC: i32 = 1;
const MIGRATION_SQL: &str = include_str!(
    "../../../src/store/persistence/migrations/20260921000002_session_judge_attempts.sql"
);

async fn pool_for(url: &str) -> anyhow::Result<PgPool> {
    Ok(PgPoolOptions::new().max_connections(2).connect(url).await?)
}

async fn new_session_with_turn(
    pool: &PgPool,
    label: &str,
    turn_at: DateTime<Utc>,
) -> anyhow::Result<(i64, i64, i64)> {
    let (project_id, session_pk, _) = bootstrap_project_session(
        pool,
        &format!("/test/att-{label}/{}", Uuid::new_v4().simple()),
    )
    .await?;
    seed_completed_request(pool, session_pk, project_id).await?;
    let turn_id = seed_unjudged_operator_turn(pool, session_pk, project_id, turn_at).await?;
    set_turn_created_at(pool, turn_id, turn_at).await?;
    Ok((project_id, session_pk, turn_id))
}

async fn set_turn_created_at(
    pool: &PgPool,
    turn_id: i64,
    created_at: DateTime<Utc>,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE memory.operator_turns SET created_at = $2 WHERE id = $1")
        .bind(turn_id)
        .bind(created_at)
        .execute(pool)
        .await?;
    Ok(())
}

async fn add_turn(
    pool: &PgPool,
    session_pk: i64,
    project_id: i64,
    at: DateTime<Utc>,
) -> anyhow::Result<()> {
    let turn_id = seed_unjudged_operator_turn(pool, session_pk, project_id, at).await?;
    set_turn_created_at(pool, turn_id, at).await
}

async fn give_verdict(
    pool: &PgPool,
    session_pk: i64,
    outcome: &str,
    judged_at: DateTime<Utc>,
) -> anyhow::Result<()> {
    let outcome_id = resolve_outcome_id(pool, outcome).await?;
    sqlx::query(
        "INSERT INTO session_judges.verdicts
             (session_id, outcome_id, quality_score, rationale, judge_model,
              rubric_version, judged_at)
         VALUES ($1, $2, 0.5::numeric(4,3), 'r', $3, $4, $5)",
    )
    .bind(session_pk)
    .bind(outcome_id)
    .bind(MODEL)
    .bind(RUBRIC)
    .bind(judged_at)
    .execute(pool)
    .await?;
    Ok(())
}

async fn give_attempt(
    pool: &PgPool,
    session_pk: i64,
    model: &str,
    at: DateTime<Utc>,
    consecutive_failures: i32,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO session_judges.attempts
             (session_id, judge_model, rubric_version, last_attempt_at,
              attempt_count, consecutive_failures)
         VALUES ($1, $2, $3, $4, 1, $5)
         ON CONFLICT (session_id, judge_model, rubric_version)
         DO UPDATE SET last_attempt_at = EXCLUDED.last_attempt_at,
                       consecutive_failures = EXCLUDED.consecutive_failures",
    )
    .bind(session_pk)
    .bind(model)
    .bind(RUBRIC)
    .bind(at)
    .bind(consecutive_failures)
    .execute(pool)
    .await?;
    Ok(())
}

async fn set_overview(pool: &PgPool, session_pk: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE sessions.sessions SET overview = 'written' WHERE id = $1")
        .bind(session_pk)
        .execute(pool)
        .await?;
    Ok(())
}

async fn selected(pool: &PgPool) -> anyhow::Result<Vec<SessionForJudgment>> {
    let settled_before = Utc::now() + Duration::hours(1);
    let query = NeedsJudgmentQuery {
        judge_model: MODEL,
        rubric_version: RUBRIC,
        settled_before,
        limit: 500,
        max_attempts_per_day: 3,
    };
    Ok(fetch_sessions_needing_judgment(pool, &query).await?)
}

async fn pick(pool: &PgPool, session_pk: i64) -> anyhow::Result<Option<SessionForJudgment>> {
    Ok(selected(pool)
        .await?
        .into_iter()
        .find(|r| r.session_db_id == session_pk))
}

#[tokio::test]
async fn unknown_outcome_with_no_new_turns_is_selected_once_then_left_alone() -> anyhow::Result<()>
{
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = pool_for(&handle.database_url).await?;
    let now = Utc::now();

    let (project_id, pk, _) = new_session_with_turn(&pool, "unk", now - Duration::hours(2)).await?;
    give_verdict(&pool, pk, "unknown", now - Duration::hours(1)).await?;

    let first = pick(&pool, pk).await?;
    assert!(
        first.is_some_and(|r| !r.needs_verdict),
        "an unknown verdict with a NULL overview and no attempt yet is picked once"
    );

    give_attempt(&pool, pk, MODEL, now - Duration::minutes(30), 0).await?;
    assert!(
        pick(&pool, pk).await?.is_none(),
        "after the attempt is recorded and no turn has arrived, the session must not be re-selected"
    );
    assert!(
        pick(&pool, pk).await?.is_none(),
        "a second poll must still leave it alone"
    );

    add_turn(&pool, pk, project_id, now - Duration::minutes(10)).await?;
    let again = pick(&pool, pk).await?;
    assert!(
        again.is_some_and(|r| r.needs_verdict),
        "a new operator turn after the last attempt must make the unknown verdict re-judgeable"
    );

    give_attempt(&pool, pk, MODEL, now - Duration::minutes(5), 0).await?;
    assert!(
        pick(&pool, pk).await?.is_none(),
        "once that attempt is recorded the session is left alone again"
    );

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn never_judged_session_is_selected_and_a_verdict_with_overview_never_is()
-> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = pool_for(&handle.database_url).await?;
    let now = Utc::now();

    let (_, fresh, _) = new_session_with_turn(&pool, "fresh", now - Duration::hours(1)).await?;
    let (project_id, done, _) =
        new_session_with_turn(&pool, "done", now - Duration::hours(3)).await?;
    give_verdict(&pool, done, "success", now - Duration::hours(2)).await?;
    set_overview(&pool, done).await?;

    let fresh_row = pick(&pool, fresh).await?;
    assert!(
        fresh_row.is_some_and(|r| r.needs_verdict),
        "a never-judged, never-attempted session must be selected for a verdict"
    );
    assert!(
        pick(&pool, done).await?.is_none(),
        "a session with a verdict and an overview must never be selected"
    );

    add_turn(&pool, done, project_id, now - Duration::minutes(1)).await?;
    assert!(
        pick(&pool, done).await?.is_none(),
        "new turns do not re-open a session whose known verdict and overview stand"
    );

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn overview_backfill_happens_once_per_model_and_rubric() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = pool_for(&handle.database_url).await?;
    let now = Utc::now();

    let (project_id, pk, _) = new_session_with_turn(&pool, "ov", now - Duration::hours(3)).await?;
    give_verdict(&pool, pk, "success", now - Duration::hours(2)).await?;

    assert!(
        pick(&pool, pk).await?.is_some_and(|r| !r.needs_verdict),
        "a known verdict with no overview is owed exactly one backfill pass"
    );

    give_attempt(&pool, pk, MODEL, now - Duration::hours(1), 0).await?;
    assert!(
        pick(&pool, pk).await?.is_none(),
        "the backfill attempt happened; an overview that stayed empty must not spin"
    );

    give_attempt(&pool, pk, "test-judge/other-model", now, 0).await?;
    assert!(
        pick(&pool, pk).await?.is_none(),
        "an attempt at another model must not disturb this model's record"
    );

    add_turn(&pool, pk, project_id, now - Duration::minutes(1)).await?;
    assert!(
        pick(&pool, pk).await?.is_some_and(|r| !r.needs_verdict),
        "a new turn after the attempt makes the overview backfill eligible again"
    );

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn attempt_at_another_model_does_not_suppress_this_model() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = pool_for(&handle.database_url).await?;
    let now = Utc::now();

    let (_, pk, _) = new_session_with_turn(&pool, "scope", now - Duration::hours(1)).await?;
    give_attempt(&pool, pk, "test-judge/other-model", now, 0).await?;

    assert!(
        pick(&pool, pk).await?.is_some_and(|r| r.needs_verdict),
        "an attempt recorded for a different model must not hide the session from this one"
    );

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn failed_attempts_back_off_exponentially_and_cap_at_a_day() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = pool_for(&handle.database_url).await?;
    let now = Utc::now();

    let (_, pk, _) = new_session_with_turn(&pool, "backoff", now - Duration::days(3)).await?;

    give_attempt(&pool, pk, MODEL, now - Duration::minutes(1), 1).await?;
    assert!(
        pick(&pool, pk).await?.is_none(),
        "one minute after a first failure the 5 minute backoff has not elapsed"
    );

    give_attempt(&pool, pk, MODEL, now - Duration::minutes(6), 1).await?;
    assert!(
        pick(&pool, pk).await?.is_some(),
        "six minutes after a first failure the session is retryable"
    );

    give_attempt(&pool, pk, MODEL, now - Duration::minutes(6), 3).await?;
    assert!(
        pick(&pool, pk).await?.is_none(),
        "three consecutive failures wait 20 minutes, so six minutes is too soon"
    );

    give_attempt(&pool, pk, MODEL, now - Duration::hours(23), 12).await?;
    assert!(
        pick(&pool, pk).await?.is_none(),
        "the backoff caps at 24 hours, so 23 hours is still too soon"
    );

    give_attempt(&pool, pk, MODEL, now - Duration::hours(25), 12).await?;
    assert!(
        pick(&pool, pk).await?.is_some(),
        "past the 24 hour cap a persistently failing session is retried"
    );

    handle.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn migration_seed_marks_unknown_verdicts_attempted_and_is_idempotent() -> anyhow::Result<()> {
    let Some(test_db) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let handle = test_db.acquire_isolated().await?;
    let pool = pool_for(&handle.database_url).await?;
    let now = Utc::now();

    let (_, unknown_pk, _) =
        new_session_with_turn(&pool, "seed-u", now - Duration::hours(4)).await?;
    give_verdict(&pool, unknown_pk, "unknown", now - Duration::hours(3)).await?;
    give_verdict(&pool, unknown_pk, "unknown", now - Duration::hours(2)).await?;
    let (_, known_pk, _) = new_session_with_turn(&pool, "seed-k", now - Duration::hours(4)).await?;
    give_verdict(&pool, known_pk, "success", now - Duration::hours(2)).await?;
    let (_, improved_pk, _) =
        new_session_with_turn(&pool, "seed-i", now - Duration::hours(4)).await?;
    give_verdict(&pool, improved_pk, "unknown", now - Duration::hours(3)).await?;
    give_verdict(&pool, improved_pk, "partial", now - Duration::hours(2)).await?;

    sqlx::raw_sql(MIGRATION_SQL).execute(&pool).await?;
    sqlx::raw_sql(MIGRATION_SQL).execute(&pool).await?;

    let rows: Vec<(i64, i32, DateTime<Utc>)> = sqlx::query_as(
        "SELECT session_id, attempt_count, last_attempt_at
           FROM session_judges.attempts
          WHERE session_id = ANY($1)
          ORDER BY session_id",
    )
    .bind(vec![unknown_pk, known_pk, improved_pk])
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        rows.len(),
        1,
        "only the session whose latest verdict is unknown is seeded"
    );
    assert_eq!(rows[0].0, unknown_pk);
    assert_eq!(
        rows[0].1, 2,
        "attempt_count seeds from the verdict row count"
    );

    assert!(
        pick(&pool, unknown_pk).await?.is_none(),
        "a seeded unknown session with no newer turn is not re-selected"
    );
    assert!(
        pick(&pool, known_pk).await?.is_some(),
        "a known-verdict session with no overview is still owed its one backfill"
    );

    handle.cleanup().await?;
    Ok(())
}
