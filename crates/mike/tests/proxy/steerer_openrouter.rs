use serde_json::json;
use wiremock::matchers::{header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use mike::llm::OpenRouterClient;

#[tokio::test]
async fn openrouter_verdict_call_and_deserialize() -> anyhow::Result<()> {
    use mike::steerer::verdict::SteererVerdict;

    let openrouter_mock = MockServer::start().await;

    let verdict_json = r#"{"fires":true,"confidence":0.85,"reason":"missing test evidence","nudge":"You claimed the tests pass, but no test command ran this turn - run the suite and show its output."}"#;
    let envelope = json!({
        "choices": [{
            "message": {
                "content": verdict_json
            }
        }]
    });

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header_exists("authorization"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(&envelope)
                .insert_header("content-type", "application/json"),
        )
        .expect(1)
        .mount(&openrouter_mock)
        .await;

    let client = OpenRouterClient::new(
        "sk-test-openrouter".to_string(),
        "google/gemini-3.5-flash-lite".to_string(),
        Some(openrouter_mock.uri()),
        None,
    )?;

    let schema = json!({
        "type": "object",
        "properties": {
            "fires": {"type": "boolean"},
            "confidence": {"type": "number"},
            "reason": {"type": "string"},
            "nudge": {"type": "string"}
        },
        "required": ["fires", "confidence", "reason", "nudge"],
        "additionalProperties": false
    });

    let verdict: SteererVerdict = client
        .verdict(
            "You are a session grounding assistant.",
            "Is this session drifting from the anchor?",
            schema,
        )
        .await?;

    assert!(verdict.fires);
    assert!((verdict.confidence - 0.85).abs() < 0.001);
    assert_eq!(verdict.reason, "missing test evidence");
    assert_eq!(
        verdict.nudge,
        "You claimed the tests pass, but no test command ran this turn - run the suite and show its output."
    );

    openrouter_mock.verify().await;
    Ok(())
}

#[tokio::test]
async fn openrouter_verdict_http_error_propagates() -> anyhow::Result<()> {
    use mike::llm::LlmError;
    use mike::steerer::verdict::SteererVerdict;

    let openrouter_mock = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
        .expect(1)
        .mount(&openrouter_mock)
        .await;

    let client = OpenRouterClient::new(
        "sk-test".to_string(),
        "google/gemini-3.5-flash-lite".to_string(),
        Some(openrouter_mock.uri()),
        None,
    )?;

    let schema = json!({
        "type": "object",
        "properties": {"fires": {"type": "boolean"}},
        "required": ["fires"],
        "additionalProperties": false
    });

    let result: Result<SteererVerdict, LlmError> = client.verdict("system", "user", schema).await;

    let err = result
        .err()
        .ok_or_else(|| anyhow::anyhow!("expected LlmError, got Ok"))?;
    match err {
        LlmError::Http { status } => {
            assert_eq!(status, 429);
        }
        other => anyhow::bail!("expected LlmError::Http, got {other:?}"),
    }

    openrouter_mock.verify().await;
    Ok(())
}
