use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::test_support::{TestVerdict, client_with_limiter, test_schema, valid_response};
use super::*;

#[tokio::test]
async fn verdict_refuses_once_ceiling_is_exhausted() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(true)))
        .expect(2)
        .mount(&server)
        .await;

    let limiter = CallLimiter::new(1000, Duration::from_secs(60), 2, Duration::from_secs(60));
    let client = client_with_limiter(&server, limiter)?;

    client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;
    client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;
    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;

    assert!(
        matches!(result, Err(LlmError::CeilingExceeded { limit: 2, .. })),
        "expected CeilingExceeded, got {result:?}"
    );
    server.verify().await;
    Ok(())
}

#[tokio::test]
async fn verdict_refuses_once_rate_limit_is_exhausted() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(true)))
        .expect(2)
        .mount(&server)
        .await;

    let limiter = CallLimiter::new(2, Duration::from_secs(60), 1000, Duration::from_secs(60));
    let client = client_with_limiter(&server, limiter)?;

    client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;
    client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;
    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;

    assert!(
        matches!(result, Err(LlmError::RateLimited { limit: 2 })),
        "expected RateLimited, got {result:?}"
    );
    server.verify().await;
    Ok(())
}

#[tokio::test]
async fn refusal_does_not_update_shared_health() -> anyhow::Result<()> {
    health::reset_for_test();
    let server = MockServer::start().await;
    let limiter = CallLimiter::new(1000, Duration::from_secs(60), 0, Duration::from_secs(60));
    let client = client_with_limiter(&server, limiter)?;

    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;
    assert!(matches!(result, Err(LlmError::CeilingExceeded { .. })));

    let snapshot = health::snapshot();
    assert_eq!(
        snapshot.consecutive_failures, 0,
        "a governor refusal must not be recorded as an upstream failure"
    );
    assert_eq!(snapshot.last_failure, None);
    Ok(())
}

#[tokio::test]
async fn rate_limit_allows_calls_again_after_the_window_resets() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(true)))
        .mount(&server)
        .await;

    let limiter = CallLimiter::new(1, Duration::from_millis(50), 1000, Duration::from_secs(60));
    let client = client_with_limiter(&server, limiter)?;

    client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;
    let refused = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;
    assert!(matches!(refused, Err(LlmError::RateLimited { .. })));

    tokio::time::sleep(Duration::from_millis(80)).await;

    client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;
    Ok(())
}

#[tokio::test]
async fn ceiling_allows_calls_again_after_the_window_resets() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(true)))
        .mount(&server)
        .await;

    let limiter = CallLimiter::new(1000, Duration::from_secs(60), 1, Duration::from_millis(50));
    let client = client_with_limiter(&server, limiter)?;

    client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;
    let refused = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;
    assert!(matches!(refused, Err(LlmError::CeilingExceeded { .. })));

    tokio::time::sleep(Duration::from_millis(80)).await;

    client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;
    Ok(())
}

#[tokio::test]
async fn ceiling_refusal_is_not_retried() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let limiter = CallLimiter::new(1000, Duration::from_secs(60), 0, Duration::from_secs(60));
    let client = client_with_limiter(&server, limiter)?;

    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;

    assert!(
        matches!(result, Err(LlmError::CeilingExceeded { limit: 0, .. })),
        "expected immediate CeilingExceeded with no HTTP traffic, got {result:?}"
    );
    Ok(())
}

#[test]
fn independently_constructed_clients_share_the_process_wide_limiter() -> anyhow::Result<()> {
    let a = OpenRouterClient::new("sk-a".to_string(), "model-a".to_string(), None, None)?;
    let b = OpenRouterClient::new("sk-b".to_string(), "model-b".to_string(), None, None)?;
    assert!(Arc::ptr_eq(&a.inner.limiter, &b.inner.limiter));
    Ok(())
}

#[test]
fn with_api_key_preserves_the_limiter() -> anyhow::Result<()> {
    let base = OpenRouterClient::new("old-key".to_string(), "model".to_string(), None, None)?;
    let rekeyed = base.with_api_key("new-key".to_string())?;
    assert!(Arc::ptr_eq(&base.inner.limiter, &rekeyed.inner.limiter));
    Ok(())
}

#[tokio::test]
async fn limiter_snapshot_reflects_recorded_calls() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(true)))
        .mount(&server)
        .await;

    let limiter = CallLimiter::new(5, Duration::from_secs(60), 10, Duration::from_secs(60));
    let client = client_with_limiter(&server, limiter)?;

    let before = client.limiter_snapshot();
    assert_eq!(before.calls_this_minute, 0);
    assert_eq!(before.calls_in_ceiling_window, 0);

    client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;

    let after = client.limiter_snapshot();
    assert_eq!(after.calls_this_minute, 1);
    assert_eq!(after.calls_in_ceiling_window, 1);
    assert_eq!(after.rate_limit_per_minute, 5);
    assert_eq!(after.ceiling_max_calls, 10);
    Ok(())
}
