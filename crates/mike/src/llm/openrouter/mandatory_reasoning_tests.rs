use reqwest::StatusCode;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::call::{is_reasoning_mandatory_error, sent_reasoning_effort_is_none};
use super::test_support::{TestVerdict, client_for, test_schema, valid_response};
use super::*;

const MANDATORY_BODY: &str =
    r#"{"error":{"message":"Reasoning is mandatory for this endpoint and cannot be disabled."}}"#;

fn effort_of(req: &wiremock::Request) -> Option<String> {
    let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
    body.pointer("/reasoning/effort")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

#[test]
fn detector_matches_mandatory_reasoning_client_error_case_insensitively() {
    assert!(is_reasoning_mandatory_error(
        StatusCode::BAD_REQUEST,
        MANDATORY_BODY
    ));
    assert!(is_reasoning_mandatory_error(
        StatusCode::UNPROCESSABLE_ENTITY,
        "REASONING IS MANDATORY for this model"
    ));
}

#[test]
fn detector_ignores_server_errors_and_unrelated_bodies() {
    assert!(!is_reasoning_mandatory_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        MANDATORY_BODY
    ));
    assert!(!is_reasoning_mandatory_error(
        StatusCode::BAD_REQUEST,
        "no eligible provider satisfies every parameter"
    ));
    assert!(!is_reasoning_mandatory_error(
        StatusCode::OK,
        MANDATORY_BODY
    ));
}

#[test]
fn sent_effort_detector_requires_exactly_none() {
    assert!(sent_reasoning_effort_is_none(
        &json!({"reasoning": {"effort": "none"}})
    ));
    assert!(!sent_reasoning_effort_is_none(
        &json!({"reasoning": {"effort": "low"}})
    ));
    assert!(!sent_reasoning_effort_is_none(&json!({"model": "x"})));
}

#[tokio::test]
async fn verdict_resends_with_low_effort_when_reasoning_is_mandatory() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(|req: &wiremock::Request| effort_of(req).as_deref() == Some("none"))
        .respond_with(ResponseTemplate::new(400).set_body_string(MANDATORY_BODY))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(|req: &wiremock::Request| effort_of(req).as_deref() == Some("low"))
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(true)))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server)?.with_runtime_settings(LlmRuntimeSettings::default());

    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;

    assert_eq!(result, TestVerdict { label: true });
    server.verify().await;
    Ok(())
}

#[tokio::test]
async fn verdict_does_not_resend_mandatory_reasoning_error_when_effort_was_not_none()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(400).set_body_string(MANDATORY_BODY))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server)?.with_runtime_settings(LlmRuntimeSettings {
        reasoning_effort: "medium".to_owned(),
        ..LlmRuntimeSettings::default()
    });

    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;

    assert!(matches!(result, Err(LlmError::Http { status: 400 })));
    server.verify().await;
    Ok(())
}

#[tokio::test]
async fn verdict_gives_up_when_low_effort_resend_also_fails() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(400).set_body_string(MANDATORY_BODY))
        .expect(2)
        .mount(&server)
        .await;

    let client = client_for(&server)?.with_runtime_settings(LlmRuntimeSettings::default());

    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;

    assert!(matches!(result, Err(LlmError::Http { status: 400 })));
    server.verify().await;
    Ok(())
}
