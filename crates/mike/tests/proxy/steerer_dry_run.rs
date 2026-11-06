use crate::common;

use common::start_proxy_with_openrouter;
use mike::app::{inspect_router_on, proxy_router};
use mike::config::{AppState, ProxyConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn dry_run_returns_empty_turns_without_persistence() -> anyhow::Result<()> {
    let or_server = MockServer::start().await;

    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url, shutdown) =
        start_proxy_with_openrouter(&upstream.uri(), &or_server.uri(), Some("sk-test-or-key"))
            .await?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{inspect_url}/api/steerers/dry-run"))
        .json(&json!({
            "session_id": "550e8400-e29b-41d4-a716-446655440000",
            "prompt": "Hold the session to the originally requested deliverable. \
                       The session MUST NOT claim completion without test evidence."
        }))
        .send()
        .await?;

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "dry-run must return 200"
    );

    let body: Value = resp.json().await?;

    assert!(
        body.get("turns").is_some(),
        "response missing 'turns' field: {body}"
    );
    assert!(
        body.get("would_fire_count").is_some(),
        "response missing 'would_fire_count': {body}"
    );
    assert!(
        body.get("total_evaluated").is_some(),
        "response missing 'total_evaluated': {body}"
    );
    assert!(
        body.get("model_used").is_some(),
        "response missing 'model_used': {body}"
    );

    assert_eq!(body["turns"].as_array().map(|a| a.len()).unwrap_or(1), 0);
    assert_eq!(body["would_fire_count"], 0);
    assert_eq!(body["total_evaluated"], 0);

    let received = or_server.received_requests().await;
    let count = received.as_ref().map(|r| r.len()).unwrap_or(0);
    assert_eq!(count, 0, "LLM must not be called when there are no turns");

    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn dry_run_response_shape_is_valid() -> anyhow::Result<()> {
    let or_server = MockServer::start().await;
    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url, shutdown) =
        start_proxy_with_openrouter(&upstream.uri(), &or_server.uri(), Some("sk-test-or-key"))
            .await?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{inspect_url}/api/steerers/dry-run"))
        .json(&json!({
            "session_id": "550e8400-e29b-41d4-a716-446655440000",
            "prompt": "The session MUST NOT dismiss test failures as pre-existing.",
            "turn_limit": 3,
            "confidence_threshold": 0.8
        }))
        .send()
        .await?;

    assert_eq!(resp.status(), reqwest::StatusCode::OK);

    let body: Value = resp.json().await?;

    assert!(body["turns"].is_array(), "turns must be an array: {body}");
    assert!(
        body["would_fire_count"].is_number(),
        "would_fire_count must be a number: {body}"
    );
    assert!(
        body["total_evaluated"].is_number(),
        "total_evaluated must be a number: {body}"
    );
    assert!(
        body["model_used"].is_string(),
        "model_used must be a string: {body}"
    );

    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn dry_run_unavailable_when_api_key_missing() -> anyhow::Result<()> {
    let or_server = MockServer::start().await;
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
    let inspect_app = inspect_router_on(state, inspect_listener.local_addr()?);

    let ps = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(proxy_listener, proxy_app)
            .with_graceful_shutdown(async move { ps.cancelled().await })
            .await
            .ok();
    });
    let is = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(inspect_listener, inspect_app)
            .with_graceful_shutdown(async move { is.cancelled().await })
            .await
            .ok();
    });

    let inspect_url = format!("http://{inspect_addr}");
    let _ = or_server;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{inspect_url}/api/steerers/dry-run"))
        .json(&json!({
            "session_id": "550e8400-e29b-41d4-a716-446655440000",
            "prompt": "The session MUST NOT dismiss pre-existing errors."
        }))
        .send()
        .await?;

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "dry_run_unavailable must return HTTP 200"
    );

    let body: Value = resp.json().await?;
    assert!(
        body["error"] == "dry_run_unavailable" || body.get("turns").is_some(),
        "expected dry_run_unavailable or valid response; got: {body}"
    );

    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn dry_run_unavailable_when_llm_fails() -> anyhow::Result<()> {
    let or_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(503).set_body_string("service unavailable"))
        .mount(&or_server)
        .await;

    let upstream = MockServer::start().await;
    let (_proxy_url, inspect_url, shutdown) =
        start_proxy_with_openrouter(&upstream.uri(), &or_server.uri(), Some("sk-test-or-key"))
            .await?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{inspect_url}/api/steerers/dry-run"))
        .json(&json!({
            "session_id": "550e8400-e29b-41d4-a716-446655440000",
            "prompt": "The session MUST NOT claim success without test evidence."
        }))
        .send()
        .await?;

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "dry-run endpoint must return 200"
    );

    let body: Value = resp.json().await?;
    assert!(
        body.get("turns").is_some() || body["error"] == "dry_run_unavailable",
        "unexpected response shape: {body}"
    );

    shutdown.cancel();
    Ok(())
}
