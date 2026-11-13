use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::test_support::{TestSink, TestVerdict, test_schema, valid_response};
use super::*;

fn client_on(
    model: &str,
    api_key: &str,
    server: &MockServer,
    sink: Arc<TestSink>,
) -> anyhow::Result<OpenRouterClient> {
    Ok(OpenRouterClient::new_with_limiter(
        api_key.to_owned(),
        model.to_owned(),
        Some(server.uri()),
        None,
        std::sync::Arc::new(super::test_support::fresh_limiter()),
    )?
    .with_usage_sink(sink)
    .with_role(LlmRole::Unattributed))
}

async fn expect_openrouter_call_for(
    server: &MockServer,
    wire_model: &str,
    times: u64,
) -> anyhow::Result<()> {
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_partial_json(json!({ "model": wire_model })))
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(true)))
        .expect(times)
        .mount(server)
        .await;
    Ok(())
}

#[tokio::test]
async fn explicit_openrouter_claude_model_goes_to_http_with_the_prefix_stripped()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    expect_openrouter_call_for(&server, "anthropic/claude-sonnet-5-5", 1).await?;
    let sink = Arc::new(TestSink::default());
    let client = client_on(
        "openrouter/anthropic/claude-sonnet-5-5",
        "sk-test",
        &server,
        sink.clone(),
    )?;

    let verdict: TestVerdict = client.verdict("sys", "user", test_schema()).await?;

    assert_eq!(verdict, TestVerdict { label: true });
    server.verify().await;
    let calls = sink.recorded();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].model, "anthropic/claude-sonnet-5-5");
    Ok(())
}

#[tokio::test]
async fn explicit_openrouter_gpt_model_goes_to_http_never_to_the_codex_cli() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    expect_openrouter_call_for(&server, "openai/gpt-6-sol", 1).await?;
    let sink = Arc::new(TestSink::default());
    let client = client_on(
        "openrouter/openai/gpt-6-sol",
        "sk-test",
        &server,
        sink.clone(),
    )?;

    let verdict: TestVerdict = client.verdict("sys", "user", test_schema()).await?;

    assert_eq!(verdict, TestVerdict { label: true });
    server.verify().await;
    assert_eq!(sink.recorded()[0].model, "openai/gpt-6-sol");
    Ok(())
}

#[tokio::test]
async fn unprefixed_openrouter_name_still_goes_to_http_unchanged() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    expect_openrouter_call_for(&server, "anthropic/claude-haiku", 1).await?;
    let sink = Arc::new(TestSink::default());
    let client = client_on("anthropic/claude-haiku", "sk-test", &server, sink)?;

    client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;

    server.verify().await;
    Ok(())
}

#[tokio::test]
async fn openrouter_model_without_a_key_is_refused_before_any_call_and_names_model_and_provider()
-> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(true)))
        .expect(0)
        .mount(&server)
        .await;
    let sink = Arc::new(TestSink::default());
    let client = client_on("openrouter/openai/gpt-6-sol", "", &server, sink.clone())?;

    let outcome = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;

    let Err(error) = outcome else {
        anyhow::bail!("a keyless OpenRouter model must be refused");
    };
    assert!(error.is_refusal(), "{error:?}");
    let message = error.to_string();
    assert!(message.contains("openrouter/openai/gpt-6-sol"), "{message}");
    assert!(message.contains("OpenRouter"), "{message}");
    server.verify().await;
    assert!(sink.recorded().is_empty());
    Ok(())
}

#[tokio::test]
async fn refused_keyless_call_does_not_count_as_an_llm_failure() -> anyhow::Result<()> {
    crate::llm::health::reset_for_test();
    let server = MockServer::start().await;
    let sink = Arc::new(TestSink::default());
    let client = client_on("google/gemini-3.8-flash", "", &server, sink)?;

    let _ = client
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;

    assert_eq!(crate::llm::health::snapshot().consecutive_failures, 0);
    Ok(())
}
