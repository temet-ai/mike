use mike::app::{inspect_router_on, proxy_router};
use mike::config::{AppState, ProxyConfig};
use mike::in_flight::register_for_tests;
use serde_json::Value;
use tokio::net::TcpListener;
use wiremock::MockServer;

async fn start_persist_disabled_proxy()
-> anyhow::Result<(String, AppState, tokio_util::sync::CancellationToken)> {
    let upstream = MockServer::start().await;
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        &upstream.uri(),
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "0",
    ])?;
    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_app = proxy_router(state.clone());
    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_addr = inspect_listener.local_addr()?;
    let inspect_app = inspect_router_on(state.clone(), inspect_listener.local_addr()?);

    let proxy_shutdown = shutdown.clone();
    tokio::spawn(async move {
        let _ = axum::serve(proxy_listener, proxy_app)
            .with_graceful_shutdown(async move { proxy_shutdown.cancelled().await })
            .await;
    });
    let inspect_shutdown = shutdown.clone();
    tokio::spawn(async move {
        let _ = axum::serve(inspect_listener, inspect_app)
            .with_graceful_shutdown(async move { inspect_shutdown.cancelled().await })
            .await;
    });

    Ok((format!("http://{inspect_addr}"), state, shutdown))
}

#[tokio::test]
async fn list_requests_no_persistence_excludes_in_flight_streaming_request() -> anyhow::Result<()> {
    let (inspect_url, state, shutdown) = start_persist_disabled_proxy().await?;

    let synthetic_id = register_for_tests(&state, "synthetic-session".to_string());
    assert_eq!(
        state.in_flight_len(),
        1,
        "registry must hold the synthetic record before the GET"
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?;
    assert_eq!(
        resp.status().as_u16(),
        200,
        "list_requests must succeed under persistence-disabled config"
    );
    let body: Value = resp.json().await?;
    let arr = body
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("response must be a JSON array; got {body:?}"))?;
    let surfaced_ids: Vec<u64> = arr
        .iter()
        .filter_map(|row| row.get("id").and_then(|v| v.as_u64()))
        .collect();
    assert!(
        !surfaced_ids.contains(&synthetic_id),
        "in-flight id {synthetic_id} must NOT appear in /api/requests; saw {surfaced_ids:?}"
    );

    shutdown.cancel();
    Ok(())
}
