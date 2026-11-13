use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::test_support::{
    TestSink, TestVerdict, client_for, client_with_limiter, response_with_usage, test_schema,
    valid_response,
};
use super::*;

#[tokio::test]
async fn verdict_records_successful_call_with_usage() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_with_usage(true, None)))
        .mount(&server)
        .await;

    let sink = Arc::new(TestSink::default());
    let client = client_for(&server)?
        .with_usage_sink(sink.clone())
        .with_role(LlmRole::JudgeWorker);

    client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;

    let calls = sink.recorded();
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert_eq!(call.role, LlmRole::JudgeWorker);
    assert!(matches!(call.outcome, LlmCallOutcome::Ok));
    assert_eq!(call.generation_id.as_deref(), Some("gen-abc123"));
    assert_eq!(call.prompt_tokens, Some(120));
    assert_eq!(call.completion_tokens, Some(40));
    assert_eq!(call.reasoning_tokens, Some(30));
    assert_eq!(call.cached_tokens, Some(10));
    assert!(call.cost_usd.is_some_and(|c| (c - 0.0123).abs() < 1e-9));
    assert_eq!(call.http_status, Some(200));

    let body = call
        .request_body
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("a successful call must record its request body"))?;
    assert_eq!(body["messages"][0]["content"], json!("sys"));
    assert_eq!(body["messages"][1]["content"], json!("user"));
    Ok(())
}

#[tokio::test]
async fn verdict_refusal_writes_no_usage_row_and_is_counted_by_the_limiter() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let limiter = CallLimiter::new(1000, Duration::from_secs(60), 0, Duration::from_secs(60));
    let sink = Arc::new(TestSink::default());
    let client = client_with_limiter(&server, limiter)?
        .with_usage_sink(sink.clone())
        .with_role(LlmRole::Steerer);

    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;
    assert!(matches!(result, Err(LlmError::CeilingExceeded { .. })));

    assert!(
        sink.recorded().is_empty(),
        "a refusal is not an LLM call and must not reach llm.calls: {:?}",
        sink.recorded()
    );
    let snapshot = client.limiter_snapshot();
    assert_eq!(snapshot.ceiling_refusals_total, 1);
    assert_eq!(snapshot.refusals_by_role[LlmRole::Steerer.index()], 1);
    assert_eq!(
        server.received_requests().await.unwrap_or_default().len(),
        0,
        "a refused call must never reach the network"
    );
    Ok(())
}

#[tokio::test]
async fn verdict_records_truncated_call_with_usage() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(response_with_usage(true, Some("length"))),
        )
        .mount(&server)
        .await;

    let sink = Arc::new(TestSink::default());
    let client = client_for(&server)?
        .with_runtime_settings(LlmRuntimeSettings {
            max_output_tokens: 128,
            ..LlmRuntimeSettings::default()
        })
        .with_usage_sink(sink.clone())
        .with_role(LlmRole::SessionJudge);

    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;
    assert!(matches!(result, Err(LlmError::OutputTruncated { .. })));

    let calls = sink.recorded();
    assert_eq!(calls.len(), 1);
    assert!(matches!(calls[0].outcome, LlmCallOutcome::Truncated));
    assert_eq!(calls[0].completion_tokens, Some(40));
    assert!(
        calls[0].request_body.is_some(),
        "a truncated call still built and sent a body — it must be recorded"
    );
    Ok(())
}

#[tokio::test]
async fn verdict_records_untagged_call_as_unattributed() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(true)))
        .mount(&server)
        .await;

    let sink = Arc::new(TestSink::default());
    let client = client_for(&server)?.with_usage_sink(sink.clone());

    client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;

    let calls = sink.recorded();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].role, LlmRole::Unattributed);
    Ok(())
}

#[tokio::test]
async fn verdict_skips_recording_when_record_usage_is_off() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(true)))
        .mount(&server)
        .await;

    let sink = Arc::new(TestSink::default());
    let client = client_for(&server)?
        .with_usage_sink(sink.clone())
        .with_role(LlmRole::Steerer)
        .with_runtime_settings(LlmRuntimeSettings {
            record_usage: false,
            ..LlmRuntimeSettings::default()
        });

    client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;

    assert!(sink.recorded().is_empty());
    Ok(())
}

#[tokio::test]
async fn with_attribution_tags_role_and_sink_and_records_the_model_that_ran() -> anyhow::Result<()>
{
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(true)))
        .mount(&server)
        .await;

    let sink = Arc::new(TestSink::default());
    let sink_dyn: Arc<dyn LlmUsageSink> = sink.clone();
    let client = client_for(&server)?
        .with_attribution(LlmRole::Steerer, Some(&sink_dyn))
        .with_model("openai/gpt-5.6-luna".to_owned())?;

    client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;

    let calls = sink.calls.lock().unwrap_or_else(|e| e.into_inner());
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].role, LlmRole::Steerer);
    assert_eq!(calls[0].model, "openai/gpt-5.6-luna");
    Ok(())
}

#[tokio::test]
async fn with_attribution_without_sink_keeps_role_but_records_nothing() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(true)))
        .expect(1)
        .mount(&server)
        .await;

    let client = client_for(&server)?.with_attribution(LlmRole::Steerer, None);

    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;

    assert_eq!(result, TestVerdict { label: true });
    server.verify().await;
    Ok(())
}

#[tokio::test]
async fn transport_error_is_recorded_with_transport_error_outcome() -> anyhow::Result<()> {
    let sink = Arc::new(TestSink::default());
    let client = OpenRouterClient::new_with_limiter(
        "sk-test".to_string(),
        "google/gemini-3.5-flash-lite".to_string(),
        Some("http://127.0.0.1:1".to_string()),
        None,
        std::sync::Arc::new(super::test_support::fresh_limiter()),
    )?
    .with_usage_sink(sink.clone());

    let result = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;
    assert!(matches!(result, Err(LlmError::Transport(_))));

    let calls = sink.recorded();
    assert_eq!(
        u32::try_from(calls.len())?,
        MAX_ATTEMPTS,
        "each attempt that fails at the transport layer is one recorded call: {calls:?}"
    );
    assert!(
        calls
            .iter()
            .all(|call| matches!(call.outcome, LlmCallOutcome::TransportError))
    );
    Ok(())
}
