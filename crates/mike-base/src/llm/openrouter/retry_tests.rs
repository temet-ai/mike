use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::test_support::{
    TestVerdict, client_for, empty_content_response, missing_content_response, test_schema,
    valid_response,
};
use super::*;

#[tokio::test]
async fn verdict_retries_on_empty_content_then_succeeds() -> anyhow::Result<()> {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(empty_content_response()))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(true)))
        .mount(&server)
        .await;

    let client = client_for(&server)?;
    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;

    assert_eq!(result, TestVerdict { label: true });
    Ok(())
}

#[tokio::test]
async fn verdict_retries_twice_on_parse_error_then_succeeds() -> anyhow::Result<()> {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "not valid json {"}}]
        })))
        .up_to_n_times(2)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(false)))
        .mount(&server)
        .await;

    let client = client_for(&server)?;
    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;

    assert_eq!(result, TestVerdict { label: false });
    Ok(())
}

#[tokio::test]
async fn verdict_exhausts_all_attempts_and_returns_err() -> anyhow::Result<()> {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(missing_content_response()))
        .expect(3)
        .mount(&server)
        .await;

    let client = client_for(&server)?;
    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;

    assert!(result.is_err(), "expected Err after all attempts, got Ok");
    Ok(())
}

#[tokio::test]
async fn verdict_does_not_retry_on_http_400() -> anyhow::Result<()> {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server)?;
    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;

    assert!(
        matches!(result, Err(LlmError::Http { status: 400 })),
        "expected Http{{status:400}}, got {result:?}"
    );
    Ok(())
}

#[tokio::test]
async fn verdict_failure_and_recovery_update_shared_health() -> anyhow::Result<()> {
    health::reset_for_test();
    let server = MockServer::start().await;
    let before_call = chrono::Utc::now();

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(402).set_body_string("payment required"))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    let client = client_for(&server)?;
    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;
    assert!(
        matches!(result, Err(LlmError::Http { status: 402 })),
        "expected Http{{status:402}}, got {result:?}"
    );

    let after_failure = health::snapshot();
    assert_eq!(after_failure.consecutive_failures, 1);
    let failure = after_failure
        .last_failure
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("a failure was just recorded"))?;
    assert_eq!(failure.status, Some(402));
    assert!(
        failure.at >= before_call,
        "failure timestamp must be recorded at call time: {} < {before_call}",
        failure.at
    );

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(true)))
        .mount(&server)
        .await;

    let recovered = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;
    assert_eq!(recovered, TestVerdict { label: true });

    let after_recovery = health::snapshot();
    assert_eq!(after_recovery.consecutive_failures, 0);
    assert_eq!(after_recovery.last_failure, None);
    Ok(())
}
