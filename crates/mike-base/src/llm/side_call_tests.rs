use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::llm::limiter::CallLimiter;
use crate::llm::openrouter::test_support::{
    TestVerdict, client_for, client_with_limiter, test_schema, valid_response,
};

#[tokio::test]
async fn verdict_first_link_over_its_call_ceiling_falls_through_to_second() -> anyhow::Result<()> {
    let refused = answering(false).await;
    let healthy = answering(true).await;
    let minute = std::time::Duration::from_secs(60);
    let exhausted = CallLimiter::new(1000, minute, 0, minute);
    let chain = SideCallChain::new(vec![
        client_with_limiter(&refused, exhausted)?.with_model("google/first".to_owned())?,
        link(&healthy, "google/second")?,
    ]);

    let outcome = chain
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;

    assert_eq!(outcome.value, TestVerdict { label: true });
    assert_eq!(outcome.model, "google/second");
    assert_eq!(
        refused.received_requests().await.unwrap_or_default().len(),
        0
    );
    Ok(())
}

async fn answering(label: bool) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(valid_response(label)))
        .mount(&server)
        .await;
    server
}

async fn failing(status: u16) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(status))
        .mount(&server)
        .await;
    server
}

fn link(server: &MockServer, model: &str) -> anyhow::Result<OpenRouterClient> {
    Ok(client_for(server)?.with_model(model.to_owned())?)
}

#[tokio::test]
async fn verdict_first_link_500_falls_through_to_second_and_names_it() -> anyhow::Result<()> {
    let broken = failing(500).await;
    let healthy = answering(true).await;
    let chain = SideCallChain::new(vec![
        link(&broken, "google/first")?,
        link(&healthy, "google/second")?,
    ]);

    let outcome = chain
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;

    assert_eq!(outcome.value, TestVerdict { label: true });
    assert_eq!(outcome.model, "google/second");
    assert_eq!(
        broken.received_requests().await.unwrap_or_default().len(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn verdict_breaker_opens_after_three_failures_and_skips_the_link_without_a_request()
-> anyhow::Result<()> {
    let broken = failing(500).await;
    let chain = SideCallChain::new(vec![link(&broken, "google/only")?]);

    for _ in 0..3 {
        let result = chain
            .verdict::<TestVerdict>("sys", "user", test_schema())
            .await;
        assert!(matches!(result, Err(LlmError::Http { status: 500 })));
    }
    let skipped = chain
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;

    assert!(matches!(skipped, Err(LlmError::ProviderUnavailable { .. })));
    assert_eq!(
        broken.received_requests().await.unwrap_or_default().len(),
        3
    );
    Ok(())
}

#[tokio::test]
async fn verdict_every_link_failing_returns_the_last_error() -> anyhow::Result<()> {
    let first = failing(503).await;
    let second = failing(429).await;
    let chain = SideCallChain::new(vec![
        link(&first, "google/first")?,
        link(&second, "google/second")?,
    ]);

    let result = chain
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;

    assert!(matches!(result, Err(LlmError::Http { status: 429 })));
    Ok(())
}

#[tokio::test]
async fn verdict_client_error_that_blames_the_request_is_returned_without_trying_the_next()
-> anyhow::Result<()> {
    let rejecting = failing(400).await;
    let healthy = answering(true).await;
    let chain = SideCallChain::new(vec![
        link(&rejecting, "google/first")?,
        link(&healthy, "google/second")?,
    ]);

    let result = chain
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;

    assert!(matches!(result, Err(LlmError::Http { status: 400 })));
    assert!(
        healthy
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn from_models_skips_models_the_builder_cannot_build() -> anyhow::Result<()> {
    let healthy = answering(false).await;
    let models = vec!["google/keyless".to_owned(), "google/keyed".to_owned()];

    let chain = SideCallChain::from_models(&models, |model| {
        (model == "google/keyed")
            .then(|| link(&healthy, model).ok())
            .flatten()
    });

    assert_eq!(chain.len(), 1);
    let outcome = chain
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await?;
    assert_eq!(outcome.model, "google/keyed");
    Ok(())
}

#[tokio::test]
async fn verdict_empty_chain_returns_provider_unavailable() -> anyhow::Result<()> {
    let chain = SideCallChain::from_models(&[], |_| None);

    let result = chain
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;

    assert!(chain.is_empty());
    assert!(matches!(result, Err(LlmError::ProviderUnavailable { .. })));
    Ok(())
}

#[tokio::test]
async fn verdict_fallback_cap_skips_non_local_links_once_the_hour_is_spent() -> anyhow::Result<()> {
    let first = failing(500).await;
    let second = failing(500).await;
    let chain = SideCallChain::new(vec![
        link(&first, "google/first")?,
        link(&second, "google/second")?,
    ])
    .with_max_fallback_calls_per_hour(1);

    let result = chain
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;

    assert!(matches!(result, Err(LlmError::Http { status: 500 })));
    assert_eq!(first.received_requests().await.unwrap_or_default().len(), 1);
    assert!(
        second
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn verdict_local_link_with_its_call_in_flight_is_skipped_without_a_request()
-> anyhow::Result<()> {
    let local = answering(true).await;
    let chain = SideCallChain::new(vec![link(&local, "local/gemma-4-12b")?]);
    let Some(gate) = chain.links.first().and_then(|l| l.local_gate.as_ref()) else {
        anyhow::bail!("a local link must carry a gate");
    };
    let _held = gate.try_acquire()?;

    let result = chain
        .verdict::<TestVerdict>("sys", "user", test_schema())
        .await;

    assert!(matches!(result, Err(LlmError::ProviderUnavailable { .. })));
    assert!(
        local
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn note_failure_local_busy_does_not_count_toward_the_breaker() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let local = Link::new(link(&server, "local/gemma-4-12b")?);
    let busy = LlmError::ProviderUnavailable {
        model: "local/gemma-4-12b".to_owned(),
        provider: "Local server",
        missing: LOCAL_BUSY.to_owned(),
    };

    for _ in 0..5 {
        SideCallChain::note_failure(&local, &busy);
    }

    assert!(!local.skipped());
    Ok(())
}

#[test]
fn is_usage_limit_matches_the_subscription_wording_case_insensitively() {
    for text in [
        "Claude Usage Limit reached",
        "rate_limit_error",
        "5-hour limit reached",
    ] {
        assert!(
            is_usage_limit(&LlmError::Transport(text.to_owned())),
            "{text}"
        );
    }
    assert!(!is_usage_limit(&LlmError::Transport(
        "connection reset".to_owned()
    )));
}

#[tokio::test]
async fn note_failure_claude_usage_limit_opens_the_breaker_at_once() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    let claude = Link::new(link(&server, "claude/haiku")?);

    SideCallChain::note_failure(
        &claude,
        &LlmError::Transport("You've hit your usage limit".to_owned()),
    );

    assert!(claude.skipped());
    Ok(())
}
