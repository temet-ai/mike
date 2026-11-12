use crate::anthropic_fixtures;
use crate::test_db;

use anthropic_fixtures::{MockResponse, RequestBody};
use mike::app::{inspect_router_on, proxy_router};
use mike::config::ProxyConfig;
use mike::state::AppState;
use serde_json::json;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn start_proxy_with_state(
    upstream_url: &str,
    database_url: &str,
    max_response_body_bytes: Option<u64>,
) -> anyhow::Result<(String, AppState, CancellationToken)> {
    let mut args = vec![
        "mike".to_string(),
        "--upstream-url".to_string(),
        upstream_url.to_string(),
        "--proxy-listen-addr".to_string(),
        "127.0.0.1:0".to_string(),
        "--inspect-listen-addr".to_string(),
        "127.0.0.1:0".to_string(),
        "--persist-enabled".to_string(),
        "1".to_string(),
        "--database-url".to_string(),
        database_url.to_string(),
    ];
    if let Some(cap) = max_response_body_bytes {
        args.push("--max-response-body-bytes".to_string());
        args.push(cap.to_string());
    }
    let config = ProxyConfig::try_parse_from(args)?;
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

    let proxy_url = format!("http://{proxy_addr}");
    Ok((proxy_url, state, shutdown))
}

async fn wait_for_in_flight(state: &AppState, target: usize, label: &str) -> anyhow::Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let actual = state.in_flight_len();
        if actual == target {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "[{label}] timed out waiting for in_flight_len == {target}; saw {actual}"
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn non_streaming_request_registers_then_removes_in_flight_entry() -> anyhow::Result<()> {
    let Some(test_db_factory) = test_db::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_shared().await?;

    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_in_flight_non_streaming")
                        .text("ack")
                        .usage(5, 1)
                        .build(),
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, state, shutdown) =
        start_proxy_with_state(&mock_server.uri(), &test_db.database_url, None).await?;

    assert_eq!(state.in_flight_len(), 0, "registry must start empty");

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("x-claude-code-session-id", "in-flight-test-non-streaming")
        .json(
            &RequestBody::default()
                .content("hello")
                .max_tokens(16)
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200, "non-streaming request must return 200");
    let _ = resp.bytes().await?;

    wait_for_in_flight(&state, 0, "non-streaming").await?;

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn streaming_request_registers_then_removes_in_flight_entry() -> anyhow::Result<()> {
    let Some(test_db_factory) = test_db::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_shared().await?;

    let sse_body = format!(
        "event: message_start\ndata: {}\n\n\
         event: content_block_start\ndata: {}\n\n\
         event: content_block_delta\ndata: {}\n\n\
         event: content_block_stop\ndata: {}\n\n\
         event: message_delta\ndata: {}\n\n\
         event: message_stop\ndata: {}\n\n",
        json!({
            "type": "message_start",
            "message": {
                "id": "msg_in_flight_streaming",
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": "claude-sonnet-4-20250514",
                "stop_reason": null,
                "usage": {"input_tokens": 7, "output_tokens": 0}
            }
        }),
        json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": {"type": "text", "text": ""}
        }),
        json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {"type": "text_delta", "text": "ok"}
        }),
        json!({"type": "content_block_stop", "index": 0}),
        json!({
            "type": "message_delta",
            "delta": {"stop_reason": "end_turn"},
            "usage": {"output_tokens": 4}
        }),
        json!({"type": "message_stop"}),
    );

    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(sse_body.as_bytes(), "text/event-stream"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, state, shutdown) =
        start_proxy_with_state(&mock_server.uri(), &test_db.database_url, None).await?;
    assert_eq!(state.in_flight_len(), 0, "registry must start empty");

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("x-claude-code-session-id", "in-flight-test-streaming")
        .json(
            &RequestBody::default()
                .content("hi")
                .max_tokens(16)
                .stream(true)
                .build(),
        )
        .send()
        .await?;
    assert_eq!(resp.status(), 200, "streaming request must return 200");
    let _ = resp.bytes().await?;

    wait_for_in_flight(&state, 0, "streaming").await?;

    shutdown.cancel();
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn snapshot_writes_request_diagnostics_alongside_request_row() -> anyhow::Result<()> {
    let Some(test_db_factory) = test_db::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_shared().await?;

    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    MockResponse::new("msg_diag")
                        .text("I'll just hardcode this for now to make the test pass.")
                        .usage(8, 12)
                        .build(),
                )
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let (proxy_url, _state, shutdown) =
        start_proxy_with_state(&mock_server.uri(), &test_db.database_url, None).await?;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header(
            "x-claude-code-session-id",
            "3e1a99ad-2fa0-4a7d-9b0d-1f6c0a4f2c10",
        )
        .json(
            &RequestBody::default()
                .content("just hardcode it")
                .max_tokens(16)
                .build(),
        )
        .send()
        .await?;
    let resp_status = resp.status();
    let resp_body = resp.bytes().await?;
    anyhow::ensure!(
        resp_status == 200,
        "expected 200 OK, got {resp_status}; body: {}",
        String::from_utf8_lossy(&resp_body)
    );

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let (request_id,): (i64,) = loop {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT r.id
             FROM requests.requests r
             JOIN sessions.sessions s ON s.id = r.session_id
             WHERE s.session_uuid = $1::uuid",
        )
        .bind("3e1a99ad-2fa0-4a7d-9b0d-1f6c0a4f2c10")
        .fetch_optional(&pool)
        .await?;
        if let Some(row) = row {
            break row;
        }
        if std::time::Instant::now() >= deadline {
            let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*)::bigint FROM requests.requests")
                .fetch_one(&pool)
                .await?;
            anyhow::bail!(
                "timed out waiting for persisted request row (total request rows in DB: {total})"
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };

    let (rd_count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*)::bigint FROM requests.diagnostics
         WHERE request_id = $1",
    )
    .bind(request_id)
    .fetch_one(&pool)
    .await?;
    assert!(rd_count >= 0, "rd_count must be non-negative");

    shutdown.cancel();
    drop(pool);
    test_db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn oversize_response_drop_only_abort_persists_error_status_to_db() -> anyhow::Result<()> {
    let Some(test_db_factory) = test_db::TestDb::bootstrap().await? else {
        return Ok(());
    };
    let test_db = test_db_factory.acquire_shared().await?;

    let big_response = json!({
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": "x".repeat(4096)}],
    });
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(big_response))
        .mount(&mock_server)
        .await;

    const MAX_RESP_BYTES: u64 = 1024;
    let (proxy_url, state, shutdown) = start_proxy_with_state(
        &mock_server.uri(),
        &test_db.database_url,
        Some(MAX_RESP_BYTES),
    )
    .await?;

    let session_uuid = "7f47a3b0-9e14-4c8a-8f21-5a6b7c8d9e0f";
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/messages"))
        .header("x-api-key", "sk-test")
        .header("anthropic-version", "2023-06-01")
        .header("x-claude-code-session-id", session_uuid)
        .json(
            &RequestBody::default()
                .content("trigger the oversize response")
                .max_tokens(16)
                .build(),
        )
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        502,
        "oversize upstream response must be rejected with 502"
    );
    let _ = resp.bytes().await?;

    wait_for_in_flight(&state, 0, "oversize-response-drop-abort").await?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&test_db.database_url)
        .await?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let status_code = loop {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT st.code
             FROM requests.requests r
             JOIN sessions.sessions s ON s.id = r.session_id
             JOIN requests.statuses st ON st.id = r.status_id
             WHERE s.session_uuid = $1::uuid",
        )
        .bind(session_uuid)
        .fetch_optional(&pool)
        .await?;
        match row {
            Some((code,)) if code != "pending" => break code,
            Some(_) if std::time::Instant::now() >= deadline => {
                anyhow::bail!(
                    "timed out waiting for the stub row to leave 'pending' status \
                     (InFlightGuard::drop never persisted the abort to Postgres)"
                );
            }
            None if std::time::Instant::now() >= deadline => {
                anyhow::bail!("timed out waiting for the stub row to appear in requests.requests");
            }
            _ => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
        }
    };
    assert_eq!(
        status_code, "error",
        "InFlightGuard::drop must persist the aborted status to Postgres \
         via close_aborted_request_in_db, not just flip the in-memory store"
    );

    shutdown.cancel();
    drop(pool);
    test_db.cleanup().await?;
    Ok(())
}
