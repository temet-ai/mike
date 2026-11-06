use crate::db_helpers;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::support::{mount_judge_stub, seed_unjudged_turns, start_proxy_with_judge};

#[tokio::test]
async fn judge_sweep_leaves_turn_untouched_on_governor_refusal() -> anyhow::Result<()> {
    let upstream_mock = MockServer::start().await;
    let or_mock = MockServer::start().await;

    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping judge_sweep_leaves_turn_untouched_on_governor_refusal"
        );
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let session_id = "82345678-f001-4f01-8f01-f01f01f01f06";
    let project_path = "test-judge-governor-refusal";

    seed_unjudged_turns(&pool, project_path, session_id, 1).await?;

    mount_judge_stub(&or_mock).await;

    let (_proxy_url, shutdown, state) =
        start_proxy_with_judge(&upstream_mock.uri(), &test_db.database_url, &or_mock.uri()).await?;

    mike::llm::exhaust_global_rate_for_tests();

    let labeled = mike::judge_worker::run_judge_once(&state).await?;
    assert_eq!(
        labeled, 0,
        "a refused sweep must label 0 turns, got {labeled}"
    );

    let calls = or_mock.received_requests().await.unwrap_or_default().len();
    assert_eq!(
        calls, 0,
        "a governor refusal must never reach the network, got {calls} call(s)"
    );

    let (failure_reason, attempt_count, next_attempt_at, outcome_id): (
        Option<String>,
        i32,
        Option<chrono::DateTime<chrono::Utc>>,
        i64,
    ) = sqlx::query_as(
        "SELECT t.judge_failure_reason, t.judge_attempt_count, t.judge_next_attempt_at, t.outcome_id
           FROM memory.operator_turns t
           JOIN sessions.sessions s ON s.id = t.session_id
          WHERE s.session_uuid = $1::text::uuid
          LIMIT 1",
    )
    .bind(session_id)
    .fetch_one(&pool)
    .await?;

    assert!(
        failure_reason.is_none(),
        "a refused turn must not get judge_failure_reason set, got {failure_reason:?}"
    );
    assert_eq!(
        attempt_count, 0,
        "a refused turn must not have its attempt counter bumped, got {attempt_count}"
    );
    assert!(
        next_attempt_at.is_none(),
        "a refused turn must not get a backoff deadline, got {next_attempt_at:?}"
    );
    assert_eq!(
        outcome_id, 1,
        "a refused turn must stay unknown, got outcome_id {outcome_id}"
    );

    mike::llm::reset_global_for_tests();
    let second_sweep = mike::judge_worker::run_judge_once(&state).await?;
    assert_eq!(
        second_sweep, 1,
        "the turn must be fully re-claimable once the governor has budget again, got {second_sweep}"
    );

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn judge_sweep_refusal_does_not_advance_circuit_breaker() -> anyhow::Result<()> {
    let upstream_mock = MockServer::start().await;
    let or_mock = MockServer::start().await;

    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping judge_sweep_refusal_does_not_advance_circuit_breaker"
        );
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let session_id = "92345678-f001-4f01-8f01-f01f01f01f07";
    let project_path = "test-judge-refusal-breaker";

    seed_unjudged_turns(&pool, project_path, session_id, 20).await?;

    mount_judge_stub(&or_mock).await;

    let (_proxy_url, shutdown, state) =
        start_proxy_with_judge(&upstream_mock.uri(), &test_db.database_url, &or_mock.uri()).await?;

    mike::llm::exhaust_global_rate_for_tests();

    let tripped =
        mike::judge_worker::run_judge_sweeps_and_report_breaker_tripped(&state, 10).await?;
    assert!(
        !tripped,
        "10 consecutive governor refusals must never trip the circuit breaker"
    );

    mike::llm::reset_global_for_tests();
    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn judge_sweep_leaves_turn_untouched_on_credential_failure() -> anyhow::Result<()> {
    let upstream_mock = MockServer::start().await;
    let or_mock = MockServer::start().await;

    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping judge_sweep_leaves_turn_untouched_on_credential_failure"
        );
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let session_id = "a2345678-f001-4f01-8f01-f01f01f01f08";
    let project_path = "test-judge-credential-failure";

    seed_unjudged_turns(&pool, project_path, session_id, 1).await?;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&or_mock)
        .await;

    let (_proxy_url, shutdown, state) =
        start_proxy_with_judge(&upstream_mock.uri(), &test_db.database_url, &or_mock.uri()).await?;

    let labeled = mike::judge_worker::run_judge_once(&state).await?;
    assert_eq!(
        labeled, 0,
        "a credential failure must label 0 turns, got {labeled}"
    );

    let (failure_reason, attempt_count, next_attempt_at, outcome_id): (
        Option<String>,
        i32,
        Option<chrono::DateTime<chrono::Utc>>,
        i64,
    ) = sqlx::query_as(
        "SELECT t.judge_failure_reason, t.judge_attempt_count, t.judge_next_attempt_at, t.outcome_id
           FROM memory.operator_turns t
           JOIN sessions.sessions s ON s.id = t.session_id
          WHERE s.session_uuid = $1::text::uuid
          LIMIT 1",
    )
    .bind(session_id)
    .fetch_one(&pool)
    .await?;

    assert!(
        failure_reason.is_none(),
        "a credential failure must not get judge_failure_reason set, got {failure_reason:?}"
    );
    assert_eq!(
        attempt_count, 0,
        "a credential failure must not have its attempt counter bumped, got {attempt_count}"
    );
    assert!(
        next_attempt_at.is_none(),
        "a credential failure must not get a backoff deadline, got {next_attempt_at:?}"
    );
    assert_eq!(
        outcome_id, 1,
        "a credential-failed turn must stay unknown, got outcome_id {outcome_id}"
    );

    or_mock.reset().await;
    mount_judge_stub(&or_mock).await;
    let second_sweep = mike::judge_worker::run_judge_once(&state).await?;
    assert_eq!(
        second_sweep, 1,
        "the turn must be fully re-claimable once the credential is fixed, got {second_sweep}"
    );

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn judge_sweep_credential_failure_trips_breaker_unlike_governor_refusal() -> anyhow::Result<()>
{
    let upstream_mock = MockServer::start().await;

    let Some(test_db_factory) = db_helpers::TestDb::bootstrap().await? else {
        eprintln!(
            "MIKE_TEST_ADMIN_URL unset; skipping \
             judge_sweep_credential_failure_trips_breaker_unlike_governor_refusal"
        );
        return Ok(());
    };
    let test_db = test_db_factory.acquire_isolated().await?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;

    let cred_or_mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&cred_or_mock)
        .await;

    let session_a = "b2345678-f001-4f01-8f01-f01f01f01f09";
    seed_unjudged_turns(&pool, "test-judge-cred-vs-refusal-a", session_a, 1).await?;

    let (_proxy_url_a, shutdown_a, state_a) = start_proxy_with_judge(
        &upstream_mock.uri(),
        &test_db.database_url,
        &cred_or_mock.uri(),
    )
    .await?;

    let cred_tripped =
        mike::judge_worker::run_judge_sweeps_and_report_breaker_tripped(&state_a, 10).await?;
    assert!(
        cred_tripped,
        "5+ consecutive credential failures must trip the circuit breaker (dependency is genuinely unusable)"
    );

    shutdown_a.cancel();

    let refusal_or_mock = MockServer::start().await;
    mount_judge_stub(&refusal_or_mock).await;

    let session_b = "c2345678-f001-4f01-8f01-f01f01f01f10";
    seed_unjudged_turns(&pool, "test-judge-cred-vs-refusal-b", session_b, 1).await?;

    let (_proxy_url_b, shutdown_b, state_b) = start_proxy_with_judge(
        &upstream_mock.uri(),
        &test_db.database_url,
        &refusal_or_mock.uri(),
    )
    .await?;

    mike::llm::exhaust_global_rate_for_tests();
    let refusal_tripped =
        mike::judge_worker::run_judge_sweeps_and_report_breaker_tripped(&state_b, 10).await?;
    assert!(
        !refusal_tripped,
        "10 consecutive governor refusals must never trip the circuit breaker (our own decision, not the dependency)"
    );

    mike::llm::reset_global_for_tests();
    shutdown_b.cancel();
    test_db.cleanup().await?;
    Ok(())
}
