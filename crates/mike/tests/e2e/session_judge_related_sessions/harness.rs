use mike::app::{inspect_router_on, proxy_router};
use mike::config::ProxyConfig;
use mike::state::AppState;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

pub(super) fn judge_response(
    outcome: &str,
    overview: &str,
    topics: &[&str],
    learnings: &[&str],
) -> serde_json::Value {
    let content = serde_json::json!({
        "outcome": outcome,
        "quality_score": 0.9,
        "rationale": "clean, verified work",
        "overview": overview,
        "topics": topics,
        "learnings": learnings,
    })
    .to_string();
    serde_json::json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": content },
            "finish_reason": "stop"
        }]
    })
}

pub(super) async fn start_proxy_with_state(
    upstream_url: &str,
    database_url: &str,
    or_base_url: &str,
) -> anyhow::Result<(CancellationToken, AppState)> {
    let mut config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        upstream_url,
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        database_url,
    ])?;
    config.set_openrouter_base_url(or_base_url.to_owned());
    config.set_openrouter_api_key("test-api-key".to_owned());

    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_app = proxy_router(state.clone());
    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_app = inspect_router_on(state.clone(), inspect_listener.local_addr()?);

    let proxy_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(proxy_listener, proxy_app)
            .with_graceful_shutdown(async move { proxy_shutdown.cancelled().await })
            .await
            .ok();
    });
    let inspect_shutdown = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(inspect_listener, inspect_app)
            .with_graceful_shutdown(async move { inspect_shutdown.cancelled().await })
            .await
            .ok();
    });

    Ok((shutdown, state))
}
