use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;

async fn rerank_row(state: &AppState) -> anyhow::Result<CapabilityRow> {
    let Json(payload) = capabilities(State(state.clone())).await;
    payload
        .capabilities
        .into_iter()
        .find(|row| row.id == "rerank")
        .ok_or_else(|| anyhow::anyhow!("capabilities response must carry a rerank row"))
}

async fn state() -> anyhow::Result<AppState> {
    AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await
}

#[tokio::test]
async fn rerank_row_when_disabled_is_off_and_names_the_settings_page() -> anyhow::Result<()> {
    let row = rerank_row(&state().await?).await?;

    assert_eq!(row.state, CapabilityState::Off);
    assert!(row.fix.contains("/settings/embedding"), "{}", row.fix);
    Ok(())
}

#[tokio::test]
async fn rerank_row_when_the_server_answered_is_active() -> anyhow::Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let state = state().await?;
    let service = state.rerank_service();
    service.apply_setting("rerank.enabled", &json!(true));
    service.apply_setting("rerank.url", &json!(server.uri()));
    service.health().await;

    let row = rerank_row(&state).await?;

    assert_eq!(row.state, CapabilityState::Active, "{}", row.detail);
    assert!(row.fix.is_empty());
    Ok(())
}

#[tokio::test]
async fn rerank_row_when_the_server_stops_answering_is_degraded_with_a_remedy() -> anyhow::Result<()>
{
    let state = state().await?;
    let service = state.rerank_service();
    service.apply_setting("rerank.enabled", &json!(true));
    service.apply_setting("rerank.url", &json!("http://127.0.0.1:1"));
    service.health().await;

    let row = rerank_row(&state).await?;

    assert_eq!(row.state, CapabilityState::Degraded);
    assert!(row.detail.contains("http://127.0.0.1:1"), "{}", row.detail);
    assert!(!row.fix.is_empty());
    Ok(())
}

#[tokio::test]
async fn rerank_row_for_a_remote_url_without_egress_permission_is_inert() -> anyhow::Result<()> {
    let state = state().await?;
    let service = state.rerank_service();
    service.apply_setting("rerank.enabled", &json!(true));
    service.apply_setting("rerank.url", &json!("http://omit:11435"));

    let row = rerank_row(&state).await?;

    assert_eq!(row.state, CapabilityState::Inert);
    assert!(
        row.fix.contains("semantic.allow_remote_embedding"),
        "{}",
        row.fix
    );
    Ok(())
}
