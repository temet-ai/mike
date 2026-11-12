use crate::anthropic_fixtures;

use anthropic_fixtures::{MockResponse, RequestBody};
use mike::app::{inspect_router_on, proxy_router};
use mike::config::register_in_flight_for_tests as register_for_tests;
use mike::config::{AppState, ProxyConfig};
use tokio::net::TcpListener;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn start_proxy_with_capacity(
    upstream_url: &str,
    max_in_flight: u64,
) -> anyhow::Result<(String, AppState, tokio_util::sync::CancellationToken)> {
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        upstream_url,
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "0",
        "--max-in-flight",
        &max_in_flight.to_string(),
    ])?;
    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_addr = proxy_listener.local_addr()?;
    let proxy_app = proxy_router(state.clone());

    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
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

    Ok((format!("http://{proxy_addr}"), state, shutdown))
}

#[tokio::test]
async fn proxy_returns_503_with_retry_after_when_in_flight_registry_is_saturated()
-> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_capacity_test")
                        .text("ok")
                        .usage(1, 1)
                        .build(),
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, state, shutdown) = start_proxy_with_capacity(&mock_server.uri(), 3).await?;
    assert_eq!(state.in_flight_len(), 0, "registry must start empty");

    for i in 0..3 {
        let _id = register_for_tests(&state, format!("synthetic-{i}"));
    }
    assert_eq!(
        state.in_flight_len(),
        3,
        "registry must hold 3 synthetic records before the 503 probe"
    );

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header(
            "x-claude-code-session-id",
            "a1b2c3d4-0000-4000-8000-000000000001",
        )
        .json(&RequestBody::default().content("hi").max_tokens(8).build())
        .send()
        .await?;

    assert_eq!(
        resp.status().as_u16(),
        503,
        "saturated registry must produce 503 Service Unavailable"
    );
    let retry_after = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    assert_eq!(
        retry_after.as_deref(),
        Some("1"),
        "503 response must carry `Retry-After: 1`"
    );
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    assert_eq!(
        content_type.as_deref(),
        Some("application/json"),
        "503 response body must be JSON"
    );
    let body_text = resp.text().await?;
    assert!(
        body_text.contains("in-flight registry saturated"),
        "503 body must surface the saturation reason; got {body_text:?}"
    );

    shutdown.cancel();
    Ok(())
}
