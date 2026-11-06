use crate::db_helpers;

use sqlx::PgPool;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::support::{
    count_judged_turns, mount_judge_stub, seed_unjudged_turns, start_proxy_with_judge,
};

#[tokio::test]
async fn judge_sweep_records_failure_reason_on_permanent_error() -> anyhow::Result<()> {
    let upstream_mock = MockServer::start().await;
    let or_mock = MockServer::start().await;

    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping judge_sweep_records_failure_reason_on_permanent_error"
        );
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let session_id = "32345678-f001-4f01-8f01-f01f01f01f03";
    let project_path = "test-judge-failure-reason";

    seed_unjudged_turns(&pool, project_path, session_id, 1).await?;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(400))
        .mount(&or_mock)
        .await;

    let (_proxy_url, shutdown, state) =
        start_proxy_with_judge(&upstream_mock.uri(), &test_db.database_url, &or_mock.uri()).await?;

    let labeled = mike::judge_worker::run_judge_once(&state).await?;
    assert_eq!(
        labeled, 0,
        "a permanently-failing judge call must label 0 turns, got {labeled}"
    );

    let reason: Option<String> = sqlx::query_scalar(
        "SELECT t.judge_failure_reason
           FROM memory.operator_turns t
           JOIN sessions.sessions s ON s.id = t.session_id
          WHERE s.session_uuid = $1::text::uuid
          LIMIT 1",
    )
    .bind(session_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        reason.as_deref(),
        Some("permanent"),
        "the turn must record judge_failure_reason = 'permanent', got {reason:?}"
    );

    let (never_judged, judged_inconclusive, judge_failed): (i64, i64, i64) = sqlx::query_as(
        "SELECT
             count(*) FILTER (WHERE t.outcome_id = 1 AND t.judge_rubric_version IS NULL
                                AND t.judge_failure_reason IS NULL),
             count(*) FILTER (WHERE t.outcome_id = 1 AND t.judge_rubric_version IS NOT NULL),
             count(*) FILTER (WHERE t.judge_failure_reason IS NOT NULL)
           FROM memory.operator_turns t
           JOIN sessions.sessions s ON s.id = t.session_id
          WHERE s.session_uuid = $1::text::uuid",
    )
    .bind(session_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        (never_judged, judged_inconclusive, judge_failed),
        (0, 0, 1),
        "the three-state query must classify the seeded turn as judge_failed only"
    );

    let second_sweep = mike::judge_worker::run_judge_once(&state).await?;
    assert_eq!(
        second_sweep, 0,
        "a permanently-failed turn must not be reclaimed by a later sweep, got {second_sweep}"
    );

    let (attempt_count, next_attempt_at): (i32, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as(
            "SELECT t.judge_attempt_count, t.judge_next_attempt_at
               FROM memory.operator_turns t
               JOIN sessions.sessions s ON s.id = t.session_id
              WHERE s.session_uuid = $1::text::uuid
              LIMIT 1",
        )
        .bind(session_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        attempt_count, 1,
        "a permanent failure still increments the attempt counter for legibility"
    );
    assert!(
        next_attempt_at.is_none(),
        "a permanent failure must not get a backoff deadline — it is excluded outright, \
         not backed off; got {next_attempt_at:?}"
    );

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn judge_sweep_stamps_outcome_after_transient_429() -> anyhow::Result<()> {
    let upstream_mock = MockServer::start().await;
    let or_mock = MockServer::start().await;

    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let session_id = "22345678-f001-4f01-8f01-f01f01f01f02";
    let project_path = "test-judge-429-retry";

    seed_unjudged_turns(&pool, project_path, session_id, 1).await?;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .mount(&or_mock)
        .await;
    mount_judge_stub(&or_mock).await;

    let (_proxy_url, shutdown, state) =
        start_proxy_with_judge(&upstream_mock.uri(), &test_db.database_url, &or_mock.uri()).await?;

    let labeled = mike::judge_worker::run_judge_once(&state).await?;
    assert_eq!(
        labeled, 1,
        "a 429 followed by a success must still label the turn, got {labeled}"
    );

    let judged = count_judged_turns(&pool, session_id).await?;
    assert_eq!(
        judged, 1,
        "the turn's outcome must be stamped from the post-retry verdict, not left unknown"
    );

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

async fn fetch_backoff_state(
    pool: &PgPool,
    session_id: &str,
) -> anyhow::Result<(Option<String>, i32, Option<f64>)> {
    sqlx::query_as(
        "SELECT t.judge_failure_reason,
                    t.judge_attempt_count,
                    EXTRACT(EPOCH FROM (t.judge_next_attempt_at - NOW()))::float8
               FROM memory.operator_turns t
               JOIN sessions.sessions s ON s.id = t.session_id
              WHERE s.session_uuid = $1::text::uuid
              LIMIT 1",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await
    .map_err(anyhow::Error::from)
}

async fn fetch_attempt_count_and_backoff_delay(
    pool: &PgPool,
    session_id: &str,
) -> anyhow::Result<(i32, Option<f64>)> {
    sqlx::query_as(
        "SELECT t.judge_attempt_count,
                EXTRACT(EPOCH FROM (t.judge_next_attempt_at - NOW()))::float8
           FROM memory.operator_turns t
           JOIN sessions.sessions s ON s.id = t.session_id
          WHERE s.session_uuid = $1::text::uuid
          LIMIT 1",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await
    .map_err(anyhow::Error::from)
}

async fn force_backoff_window_elapsed(pool: &PgPool, session_id: &str) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE memory.operator_turns t
            SET judge_next_attempt_at = NOW() - INTERVAL '1 second'
           FROM sessions.sessions s
          WHERE s.id = t.session_id AND s.session_uuid = $1::text::uuid",
    )
    .bind(session_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn judge_sweep_backs_off_transient_exhausted_turn_with_growing_delay() -> anyhow::Result<()> {
    let upstream_mock = MockServer::start().await;
    let or_mock = MockServer::start().await;

    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping \
             judge_sweep_backs_off_transient_exhausted_turn_with_growing_delay"
        );
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let session_id = "42345678-f001-4f01-8f01-f01f01f01f04";
    let project_path = "test-judge-transient-backoff";

    seed_unjudged_turns(&pool, project_path, session_id, 1).await?;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&or_mock)
        .await;

    let (_proxy_url, shutdown, state) =
        start_proxy_with_judge(&upstream_mock.uri(), &test_db.database_url, &or_mock.uri()).await?;

    let labeled = mike::judge_worker::run_judge_once(&state).await?;
    assert_eq!(
        labeled, 0,
        "an exhausted transient failure must label 0 turns, got {labeled}"
    );

    let (reason, attempt_count, secs_until_next) = fetch_backoff_state(&pool, session_id).await?;
    assert_eq!(reason.as_deref(), Some("transient_exhausted"));
    assert_eq!(
        attempt_count, 1,
        "the first failure must set judge_attempt_count = 1, got {attempt_count}"
    );
    let first_delay = secs_until_next.ok_or_else(|| {
        anyhow::anyhow!("a transient_exhausted failure must set judge_next_attempt_at")
    })?;
    assert!(
        (250.0..=350.0).contains(&first_delay),
        "first backoff must land near the 300s base, got {first_delay}s"
    );

    let second_sweep = mike::judge_worker::run_judge_once(&state).await?;
    assert_eq!(
        second_sweep, 0,
        "a turn inside its backoff window must not be reclaimed, got {second_sweep}"
    );

    force_backoff_window_elapsed(&pool, session_id).await?;

    let third_sweep = mike::judge_worker::run_judge_once(&state).await?;
    assert_eq!(
        third_sweep, 0,
        "the mock still always fails — the turn must fail again, not succeed, got {third_sweep}"
    );

    let (attempt_count_2, secs_until_next_2) =
        fetch_attempt_count_and_backoff_delay(&pool, session_id).await?;
    assert_eq!(
        attempt_count_2, 2,
        "a second consecutive failure must bump judge_attempt_count to 2, got {attempt_count_2}"
    );
    let second_delay = secs_until_next_2
        .ok_or_else(|| anyhow::anyhow!("still transient_exhausted after the second failure"))?;
    assert!(
        (550.0..=650.0).contains(&second_delay),
        "the second failure's backoff must roughly double (300 * 2^1 = 600s), got {second_delay}s"
    );
    assert!(
        second_delay > first_delay,
        "backoff must grow with repeated failures, not stay flat: first={first_delay}s \
         second={second_delay}s"
    );

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}
