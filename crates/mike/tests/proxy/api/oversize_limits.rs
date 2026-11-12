use crate::anthropic_fixtures;

use anthropic_fixtures::RequestBody;
use mike::app::{inspect_router_on, proxy_router};
use mike::config::ProxyConfig;
use mike::state::AppState;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn spawn_proxy_and_inspect(
    config: ProxyConfig,
) -> anyhow::Result<(String, CancellationToken)> {
    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_addr = proxy_listener.local_addr()?;
    let proxy_app = proxy_router(state.clone());

    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_app = inspect_router_on(state, inspect_listener.local_addr()?);

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

    Ok((format!("http://{proxy_addr}"), shutdown))
}

#[tokio::test]
async fn proxy_handler_rejects_oversize_body_with_413() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&mock_server)
        .await;

    const MAX_BYTES: u64 = 1024;
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        &mock_server.uri(),
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "0",
        "--max-request-body-bytes",
        &MAX_BYTES.to_string(),
    ])?;
    let (proxy_url, shutdown) = spawn_proxy_and_inspect(config).await?;
    let client = reqwest::Client::new();

    let huge = "x".repeat(4096);
    let body = RequestBody::default().content(huge).max_tokens(16).build();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test-cap")
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await?;
    assert_eq!(
        resp.status().as_u16(),
        413,
        "oversize request must return 413"
    );
    let body: Value = resp.json().await?;
    let err = body["error"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("error string missing: {body}"))?;
    assert!(
        err.contains(&MAX_BYTES.to_string()),
        "413 body must mention the configured cap, got {err}"
    );

    let received = mock_server.received_requests().await;
    let received = received.ok_or_else(|| anyhow::anyhow!("wiremock dropped request log"))?;
    assert!(
        received.is_empty(),
        "upstream must NOT receive an oversize request, got {} request(s)",
        received.len()
    );

    let small = RequestBody::default().content("hi").max_tokens(1).build();
    let small_resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test-cap")
        .header("anthropic-version", "2023-06-01")
        .json(&small)
        .send()
        .await?;
    assert_eq!(
        small_resp.status().as_u16(),
        200,
        "under-cap request must still pass through"
    );

    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn proxy_handler_rejects_oversize_upstream_response_with_502() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    let big_response = json!({
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": "x".repeat(4096)}],
    });
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(big_response))
        .mount(&mock_server)
        .await;

    const MAX_RESP_BYTES: u64 = 1024;
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        &mock_server.uri(),
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "0",
        "--max-response-body-bytes",
        &MAX_RESP_BYTES.to_string(),
    ])?;
    let (proxy_url, shutdown) = spawn_proxy_and_inspect(config).await?;

    let client = reqwest::Client::new();
    let body = RequestBody::default().content("hi").max_tokens(16).build();
    assert_oversize_response_yields_structured_502(&client, &proxy_url, &body, MAX_RESP_BYTES)
        .await?;

    let small_shutdown =
        assert_undersize_response_still_passes(&client, &body, MAX_RESP_BYTES).await?;

    shutdown.cancel();
    small_shutdown.cancel();
    Ok(())
}

async fn assert_oversize_response_yields_structured_502(
    client: &reqwest::Client,
    proxy_url: &str,
    body: &Value,
    max_resp_bytes: u64,
) -> anyhow::Result<()> {
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test-cap")
        .header("anthropic-version", "2023-06-01")
        .header(
            "x-claude-code-session-id",
            "f6a7b8c9-d0e1-2345-fab0-345678901235",
        )
        .json(body)
        .send()
        .await?;
    assert_eq!(
        resp.status().as_u16(),
        502,
        "oversize upstream response must yield 502"
    );
    let parsed: Value = resp.json().await?;
    assert_eq!(
        parsed["truncated"],
        Value::Bool(false),
        "structured truncated flag must be present and explicitly false (no silent slicing): {parsed}"
    );
    let err = parsed["error"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("error string missing: {parsed}"))?;
    assert!(
        err.contains(&max_resp_bytes.to_string()),
        "502 body must mention the configured cap, got {err}"
    );
    Ok(())
}

async fn assert_undersize_response_still_passes(
    client: &reqwest::Client,
    body: &Value,
    max_resp_bytes: u64,
) -> anyhow::Result<CancellationToken> {
    let small_mock = MockServer::start().await;
    let small_response = json!({
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": "hi"}],
    });
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(small_response))
        .mount(&small_mock)
        .await;
    let small_config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        &small_mock.uri(),
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "0",
        "--max-response-body-bytes",
        &max_resp_bytes.to_string(),
    ])?;
    let (small_url, small_shutdown) = spawn_proxy_and_inspect(small_config).await?;
    let small_resp = client
        .post(format!("{small_url}/v1/messages"))
        .header("x-api-key", "sk-test-cap")
        .header("anthropic-version", "2023-06-01")
        .header(
            "x-claude-code-session-id",
            "a7b8c9d0-e1f2-3456-ab01-456789012346",
        )
        .json(body)
        .send()
        .await?;
    assert_eq!(
        small_resp.status().as_u16(),
        200,
        "under-cap response must still pass through"
    );
    Ok(small_shutdown)
}
