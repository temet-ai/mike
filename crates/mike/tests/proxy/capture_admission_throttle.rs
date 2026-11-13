use crate::anthropic_fixtures;

use anthropic_fixtures::{MockResponse, RequestBody, text_block};
use mike::app::{inspect_router_on, proxy_router};
use mike::config::ProxyConfig;
use mike::state::AppState;
use mike::store::test_support::{TestDb, TestDbHandle};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn start_proxy(
    upstream_url: &str,
) -> anyhow::Result<(
    String,
    String,
    AppState,
    tokio_util::sync::CancellationToken,
)> {
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
    ])?;
    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_addr = proxy_listener.local_addr()?;
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

    Ok((
        format!("http://{proxy_addr}"),
        format!("http://{inspect_addr}"),
        state,
        shutdown,
    ))
}

#[tokio::test]
async fn throttled_request_forwards_and_increments_capture_throttled_counter() -> anyhow::Result<()>
{
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_throttle_test")
                        .text("ok")
                        .usage(1, 1)
                        .build(),
                )
                .insert_header("content-type", "application/json"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let (proxy_url, inspect_url, state, shutdown) = start_proxy(&mock_server.uri()).await?;

    state.set_capture_admission_throttled_for_tests(true);

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header(
            "x-claude-code-session-id",
            "a1b2c3d4-0000-4000-8000-000000000002",
        )
        .json(&RequestBody::default().content("hi").max_tokens(8).build())
        .send()
        .await?;

    assert_eq!(
        resp.status().as_u16(),
        200,
        "capture throttling degrades inspection only — the client-visible \
         call to Anthropic must still succeed"
    );

    let metrics_body = client
        .get(format!("{inspect_url}/api/metrics"))
        .send()
        .await?
        .text()
        .await?;
    assert!(
        metrics_body.contains("proxy_uncaptured_requests_total{reason=\"capture_throttled\"} 1"),
        "the capture_throttled label must have incremented; /api/metrics body: {metrics_body}"
    );
    assert!(
        metrics_body.contains("proxy_capture_throttle_override_total 0"),
        "an unmutated throttled request must leave the override counter at 0; \
         /api/metrics body: {metrics_body}"
    );

    let requests: Value = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?
        .json()
        .await?;
    assert_eq!(
        requests.as_array().map(Vec::len).unwrap_or(usize::MAX),
        0,
        "an unmutated throttled request must NOT be captured: {requests:?}"
    );

    shutdown.cancel();
    Ok(())
}

#[tokio::test]
async fn non_throttled_request_does_not_increment_capture_throttled_counter() -> anyhow::Result<()>
{
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_no_throttle_test")
                        .text("ok")
                        .usage(1, 1)
                        .build(),
                )
                .insert_header("content-type", "application/json"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let (proxy_url, inspect_url, _state, shutdown) = start_proxy(&mock_server.uri()).await?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header(
            "x-claude-code-session-id",
            "a1b2c3d4-0000-4000-8000-000000000003",
        )
        .json(&RequestBody::default().content("hi").max_tokens(8).build())
        .send()
        .await?;
    assert_eq!(resp.status().as_u16(), 200);

    let metrics_body = client
        .get(format!("{inspect_url}/api/metrics"))
        .send()
        .await?
        .text()
        .await?;
    assert!(
        metrics_body.contains("proxy_uncaptured_requests_total{reason=\"capture_throttled\"} 0"),
        "an un-throttled request must leave the capture_throttled counter \
         at its pre-registered 0; /api/metrics body: {metrics_body}"
    );

    shutdown.cancel();
    Ok(())
}

async fn start_proxy_with_rules_and_state(
    upstream_url: &str,
    rules: &[Value],
) -> anyhow::Result<
    Option<(
        String,
        String,
        AppState,
        TestDbHandle,
        tokio_util::sync::CancellationToken,
    )>,
> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let test_db = factory.acquire_isolated().await?;

    let config = ProxyConfig::try_parse_from([
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
        test_db.database_url.as_str(),
    ])?;

    let state = AppState::new(config).await?;
    let shutdown = state.shutdown();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await?;
    let proxy_addr = proxy_listener.local_addr()?;
    let proxy_app = proxy_router(state.clone());

    let inspect_listener = TcpListener::bind("127.0.0.1:0").await?;
    let inspect_addr = inspect_listener.local_addr()?;
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
    tokio::task::yield_now().await;

    let proxy_url = format!("http://{proxy_addr}");
    let inspect_url = format!("http://{inspect_addr}");

    let client = reqwest::Client::new();
    for rule in rules {
        let resp = client
            .post(format!("{inspect_url}/api/filter/rules"))
            .json(rule)
            .send()
            .await?;
        let status = resp.status();
        anyhow::ensure!(
            status == reqwest::StatusCode::CREATED,
            "filter rule seed must return 201, got {status}: {}",
            resp.text().await?
        );
    }

    Ok(Some((proxy_url, inspect_url, state, test_db, shutdown)))
}

#[tokio::test]
async fn throttled_but_mutated_request_is_captured_and_persisted() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_throttle_mutated_test")
                        .text("ok")
                        .usage(1, 1)
                        .build(),
                )
                .insert_header("content-type", "application/json"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let drop_system_reminder_rule =
        json!({"kind": "xml_tag", "value": "system-reminder", "action": "drop"});
    let Some((proxy_url, inspect_url, state, test_db, shutdown)) =
        start_proxy_with_rules_and_state(&mock_server.uri(), &[drop_system_reminder_rule]).await?
    else {
        return Ok(());
    };

    state.set_capture_admission_throttled_for_tests(true);

    let session_id = uuid::Uuid::new_v4().to_string();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("x-claude-code-session-id", &session_id)
        .json(
            &RequestBody::default()
                .max_tokens(8)
                .system(json!([text_block(
                    "<system-reminder>drop me</system-reminder>"
                )]))
                .content("hi")
                .build(),
        )
        .send()
        .await?;
    assert_eq!(
        resp.status().as_u16(),
        200,
        "the client-visible call must still succeed under the throttle"
    );

    assert_drop_rule_fired_upstream(&mock_server).await?;
    assert_request_visible_via_inspect(&client, &inspect_url).await?;
    assert_request_row_persisted(&test_db, &session_id).await?;
    assert_override_counter_and_not_capture_throttled(&client, &inspect_url).await?;

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

async fn assert_drop_rule_fired_upstream(mock_server: &MockServer) -> anyhow::Result<()> {
    let received = mock_server
        .received_requests()
        .await
        .ok_or_else(|| anyhow::anyhow!("no requests received"))?;
    assert_eq!(received.len(), 1);
    let upstream_body: Value = serde_json::from_slice(&received[0].body)?;
    assert!(
        upstream_body["system"]
            .as_array()
            .map(Vec::is_empty)
            .unwrap_or(false),
        "the drop rule must have fired against the upstream body: {upstream_body}"
    );
    Ok(())
}

async fn assert_request_visible_via_inspect(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<()> {
    let requests: Value = client
        .get(format!("{inspect_url}/api/requests"))
        .send()
        .await?
        .json()
        .await?;
    let requests = requests
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("requests must be array"))?;
    assert_eq!(
        requests.len(),
        1,
        "a mutated request must be captured despite the throttle: {requests:?}"
    );
    Ok(())
}

async fn assert_request_row_persisted(
    test_db: &TestDbHandle,
    session_id: &str,
) -> anyhow::Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&test_db.database_url)
        .await?;
    let row_count = mike::store::test_support::request_row_count(&pool, session_id).await?;
    pool.close().await;
    assert_eq!(
        row_count, 1,
        "the mutated+throttled request must have a persisted requests.requests row"
    );
    Ok(())
}

async fn assert_override_counter_and_not_capture_throttled(
    client: &reqwest::Client,
    inspect_url: &str,
) -> anyhow::Result<()> {
    let metrics_body = client
        .get(format!("{inspect_url}/api/metrics"))
        .send()
        .await?
        .text()
        .await?;
    assert!(
        metrics_body.contains("proxy_capture_throttle_override_total 1"),
        "the override counter must have incremented; /api/metrics body: {metrics_body}"
    );
    assert!(
        metrics_body.contains("proxy_uncaptured_requests_total{reason=\"capture_throttled\"} 0"),
        "a mutated request must NOT count against capture_throttled — it was captured; \
         /api/metrics body: {metrics_body}"
    );
    Ok(())
}
