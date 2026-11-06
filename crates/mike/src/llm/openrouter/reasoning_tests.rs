use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::test_support::{
    TestVerdict, client_for, response_with_usage, test_schema, valid_response,
};
use super::*;

#[tokio::test]
async fn verdict_sends_configured_reasoning_effort_and_max_tokens() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(wiremock::matchers::body_partial_json(json!({
            "reasoning": {"effort": "low"},
            "max_tokens": 500
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_with_usage(true, None)))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server)?.with_runtime_settings(LlmRuntimeSettings {
        reasoning_effort: "low".to_owned(),
        max_output_tokens: 500,
        ..LlmRuntimeSettings::default()
    });

    client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;
    server.verify().await;
    Ok(())
}

#[tokio::test]
async fn verdict_omits_reasoning_and_max_tokens_without_runtime_settings() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(|req: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
            body.get("reasoning").is_none() && body.get("max_tokens").is_none()
        })
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(true)))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server)?;
    client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;
    server.verify().await;
    Ok(())
}

#[tokio::test]
async fn verdict_returns_output_truncated_and_does_not_retry() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(response_with_usage(true, Some("length"))),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server)?.with_runtime_settings(LlmRuntimeSettings {
        max_output_tokens: 128,
        ..LlmRuntimeSettings::default()
    });

    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;
    assert!(
        matches!(
            result,
            Err(LlmError::OutputTruncated {
                max_output_tokens: 128
            })
        ),
        "expected OutputTruncated{{128}}, got {result:?}"
    );
    server.verify().await;
    Ok(())
}

#[tokio::test]
async fn verdict_retries_once_without_reasoning_on_provider_eligibility_error() -> anyhow::Result<()>
{
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(|req: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
            body.get("reasoning").is_some()
        })
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_string("no eligible provider satisfies every parameter"),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(|req: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
            body.get("reasoning").is_none()
        })
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(true)))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server)?.with_runtime_settings(LlmRuntimeSettings {
        reasoning_effort: "low".to_owned(),
        ..LlmRuntimeSettings::default()
    });

    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;
    assert_eq!(result, TestVerdict { label: true });
    server.verify().await;
    Ok(())
}

#[tokio::test]
async fn verdict_does_not_retry_eligibility_error_without_runtime_settings() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(400).set_body_string("no eligible provider for this model"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server)?;
    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;
    assert!(matches!(result, Err(LlmError::Http { status: 400 })));
    server.verify().await;
    Ok(())
}
