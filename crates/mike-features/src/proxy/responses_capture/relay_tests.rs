use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::Row;
use tower::ServiceExt;

use super::relay::upstream_url;
use super::{Upstreams, routes};

use super::relay_test_support::*;

#[tokio::test]
async fn diagnostic_off_keeps_complete_capture_and_usage_without_duplicate_bodies()
-> anyhow::Result<()> {
    let upstream = sse_upstream("/v1/responses").await;
    let Some(mut h) = harness(&format!("{}/v1", upstream.uri()), "http://127.0.0.1:1").await?
    else {
        return Ok(());
    };
    h.state
        .toggles
        .store(std::sync::Arc::new(Default::default()));
    let response = routes(h.state.clone())
        .oneshot(Request::post("/codex/responses").body(Body::from(REQUEST))?)
        .await?;
    assert_eq!(
        response.into_body().collect().await?.to_bytes().as_ref(),
        STREAM
    );
    let id = next_recorded(&mut h).await?;
    let row = sqlx::query("SELECT request_body, forwarded_body, response_body, evidence_status, usage, output_items FROM runtime.responses_exchanges WHERE id=$1")
        .bind(id).fetch_one(h.persist.pool()).await?;
    for column in ["request_body", "forwarded_body", "response_body"] {
        assert!(row.try_get::<Option<Vec<u8>>, _>(column)?.is_none());
    }
    assert_eq!(
        row.try_get::<String, _>("evidence_status")?,
        "not_collected"
    );
    assert_eq!(
        row.try_get::<Value, _>("output_items")?,
        serde_json::json!([])
    );
    assert_eq!(row.try_get::<Value, _>("usage")?["total_tokens"], 9);
    let capture = captured(&h, id).await?;
    assert_eq!(body(&h, capture.request_document_id).await?, REQUEST);
    assert_eq!(capture.sent_document_id, capture.request_document_id);
    assert_eq!(
        body(
            &h,
            capture
                .response_document_id
                .ok_or_else(|| anyhow::anyhow!("response missing"))?
        )
        .await?,
        STREAM
    );
    Ok(())
}

#[tokio::test]
async fn large_request_is_forwarded_and_captured_without_a_content_limit() -> anyhow::Result<()> {
    let upstream = sse_upstream("/v1/responses").await;
    let Some(mut h) = harness(&format!("{}/v1", upstream.uri()), "http://127.0.0.1:1").await?
    else {
        return Ok(());
    };
    let request_body = vec![b'x'; super::MAX_DIAGNOSTIC_BYTES + 1];
    let response = routes(h.state.clone())
        .oneshot(Request::post("/codex/responses").body(Body::from(request_body.clone()))?)
        .await?;
    assert_eq!(
        response.into_body().collect().await?.to_bytes().as_ref(),
        STREAM
    );
    assert_eq!(
        upstream.received_requests().await.unwrap_or_default()[0].body,
        request_body
    );
    let id = next_recorded(&mut h).await?;
    let row = sqlx::query("SELECT request_body, response_body, evidence_status FROM runtime.responses_exchanges WHERE id=$1")
        .bind(id).fetch_one(h.persist.pool()).await?;
    assert_eq!(
        row.try_get::<String, _>("evidence_status")?,
        "not_collected"
    );
    assert!(row.try_get::<Option<Vec<u8>>, _>("request_body")?.is_none());
    assert!(
        row.try_get::<Option<Vec<u8>>, _>("response_body")?
            .is_none()
    );
    let capture = captured(&h, id).await?;
    assert_eq!(body(&h, capture.request_document_id).await?, request_body);
    Ok(())
}

#[tokio::test]
async fn terminal_event_then_client_drop_is_complete_even_without_transport_eof()
-> anyhow::Result<()> {
    use futures::StreamExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let Some(mut h) = harness(&format!("http://{address}/v1"), "http://127.0.0.1:1").await? else {
        return Ok(());
    };
    let server = tokio::spawn(async move {
        let app = axum::Router::new().route(
            "/v1/responses",
            axum::routing::post(|| async {
                let bytes = futures::stream::once(async {
                    Ok::<_, std::io::Error>(axum::body::Bytes::from_static(STREAM))
                })
                .chain(futures::stream::pending());
                axum::response::Response::builder()
                    .header("content-type", "text/event-stream")
                    .body(Body::from_stream(bytes))
                    .unwrap_or_default()
            }),
        );
        axum::serve(listener, app).await
    });
    let response = routes(h.state.clone())
        .oneshot(Request::post("/codex/responses").body(Body::from(REQUEST))?)
        .await?;
    let mut body = response.into_body();
    let mut received = Vec::new();
    while received.len() < STREAM.len() {
        let frame = tokio::time::timeout(std::time::Duration::from_secs(5), body.frame())
            .await?
            .ok_or_else(|| anyhow::anyhow!("stream ended before terminal event"))??;
        if let Ok(bytes) = frame.into_data() {
            received.extend_from_slice(&bytes);
        }
    }
    drop(body);
    let id = next_recorded(&mut h).await?;
    server.abort();
    let row = sqlx::query(
        "SELECT status, usage, evidence_status FROM runtime.responses_exchanges WHERE id=$1",
    )
    .bind(id)
    .fetch_one(h.persist.pool())
    .await?;
    assert_eq!(row.try_get::<String, _>("status")?, "complete");
    assert_eq!(row.try_get::<Value, _>("usage")?["total_tokens"], 9);
    assert_eq!(
        row.try_get::<String, _>("evidence_status")?,
        "not_collected"
    );
    Ok(())
}

#[test]
fn upstream_url_replaces_the_codex_prefix_and_keeps_the_query() {
    assert_eq!(
        upstream_url(
            "https://chatgpt.com/backend-api/codex/",
            "/codex/models?client_version=1"
        ),
        "https://chatgpt.com/backend-api/codex/models?client_version=1"
    );
    assert_eq!(
        upstream_url("https://api.openai.com/v1", "/codex/responses"),
        "https://api.openai.com/v1/responses"
    );
}

#[test]
fn upstreams_from_lookup_defaults_and_trims_overrides() {
    let defaults = Upstreams::from_lookup(&|_| None);
    assert_eq!(defaults.openai, super::DEFAULT_OPENAI_UPSTREAM);
    assert_eq!(defaults.chatgpt, super::DEFAULT_CHATGPT_UPSTREAM);
    let set = Upstreams::from_lookup(&|key| {
        (key == super::CHATGPT_UPSTREAM_ENV).then(|| " http://127.0.0.1:9/x/ ".to_string())
    });
    assert_eq!(set.chatgpt, "http://127.0.0.1:9/x");
}

#[tokio::test]
async fn responses_chatgpt_sse_turn_is_relayed_byte_for_byte_and_recorded() -> anyhow::Result<()> {
    let chatgpt = sse_upstream("/backend-api/codex/responses").await;
    let base = format!("{}/backend-api/codex", chatgpt.uri());
    let Some(mut h) = harness("http://127.0.0.1:1", &base).await? else {
        return Ok(());
    };
    let session = uuid::Uuid::new_v4();
    let cwd = format!("/tmp/responses-fx-{session}");
    bind_session(&h, session).await?;
    let request = Request::post("/codex/responses")
        .header("content-type", "application/json")
        .header("authorization", "Bearer secret-token-fx")
        .header("chatgpt-account-id", "acct-fx")
        .header("session-id", session.to_string())
        .header("thread-id", session.to_string())
        .header("x-codex-turn-metadata", turn_metadata(session, &cwd))
        .body(Body::from(REQUEST))?;
    let response = routes(h.state.clone()).oneshot(request).await?;
    let status = response.status();
    let client_bytes = response.into_body().collect().await?.to_bytes();
    anyhow::ensure!(
        status == StatusCode::OK,
        "proxy returned {status}: {}",
        String::from_utf8_lossy(&client_bytes)
    );
    assert_eq!(client_bytes.as_ref(), STREAM);
    let id = next_recorded(&mut h).await?;

    let sent = chatgpt.received_requests().await.unwrap_or_default();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].body, REQUEST);
    assert_eq!(
        sent[0]
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok()),
        Some("Bearer secret-token-fx")
    );

    let row = sqlx::query(
        "SELECT request_body, forwarded_body, response_body, auth_route, transport, status,
                provider_turn_id, response_id, model, usage, output_items, upstream_request_id,
                request_headers::text AS headers, stream_events, session_id
           FROM runtime.responses_exchanges WHERE id = $1",
    )
    .bind(id)
    .fetch_one(h.persist.pool())
    .await?;
    assert!(row.try_get::<Option<Vec<u8>>, _>("request_body")?.is_none());
    let capture = captured(&h, id).await?;
    assert_eq!(body(&h, capture.request_document_id).await?, REQUEST);
    assert_eq!(row.try_get::<Option<Vec<u8>>, _>("forwarded_body")?, None);
    assert!(
        row.try_get::<Option<Vec<u8>>, _>("response_body")?
            .is_none()
    );
    assert_eq!(
        body(
            &h,
            capture
                .response_document_id
                .ok_or_else(|| anyhow::anyhow!("response missing"))?
        )
        .await?,
        STREAM
    );
    assert_eq!(row.try_get::<String, _>("auth_route")?, "chatgpt");
    assert_eq!(row.try_get::<String, _>("transport")?, "http");
    assert_eq!(row.try_get::<String, _>("status")?, "complete");
    let text = |name: &str| row.try_get::<Option<String>, _>(name);
    assert_eq!(text("provider_turn_id")?.as_deref(), Some("turn-fx"));
    assert_eq!(text("response_id")?.as_deref(), Some("resp_fx"));
    assert_eq!(text("model")?.as_deref(), Some("gpt-fx"));
    assert_eq!(text("upstream_request_id")?.as_deref(), Some("req_fx"));
    let usage = row.try_get::<Option<Value>, _>("usage")?;
    assert_eq!(
        usage.and_then(|u| u.get("total_tokens").cloned()),
        Some(Value::from(9))
    );
    let items = row.try_get::<Value, _>("output_items")?;
    assert_eq!(items.as_array().map(Vec::len), Some(0));
    assert_eq!(row.try_get::<Option<i32>, _>("stream_events")?, Some(3));
    let stored_headers = row.try_get::<String, _>("headers")?;
    assert!(!stored_headers.contains("secret-token-fx"));
    assert!(!stored_headers.contains("acct-fx"));

    let registered: String = sqlx::query_scalar(
        "SELECT r.registered_via FROM workspace.session_registrations r
           JOIN sessions.sessions s ON s.id = r.session_id
          WHERE s.session_uuid = $1",
    )
    .bind(session)
    .fetch_one(h.persist.pool())
    .await?;
    assert_eq!(registered, "passive");
    assert!(row.try_get::<Option<i64>, _>("session_id")?.is_some());
    Ok(())
}

#[tokio::test]
async fn responses_websocket_upgrade_is_refused_with_426_and_recorded() -> anyhow::Result<()> {
    let Some(mut h) = harness("http://127.0.0.1:1", "http://127.0.0.1:1").await? else {
        return Ok(());
    };
    let request = Request::get("/codex/responses")
        .header("connection", "Upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "13")
        .header(
            "sec-websocket-key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .header("authorization", "Bearer k")
        .body(Body::empty())?;
    let response = routes(h.state.clone()).oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::UPGRADE_REQUIRED);
    let id = next_recorded(&mut h).await?;
    let row = sqlx::query(
        "SELECT transport, status, auth_route, response_status FROM runtime.responses_exchanges WHERE id = $1",
    )
    .bind(id)
    .fetch_one(h.persist.pool())
    .await?;
    assert_eq!(row.try_get::<String, _>("transport")?, "websocket_refused");
    assert_eq!(row.try_get::<String, _>("status")?, "refused");
    assert_eq!(row.try_get::<String, _>("auth_route")?, "api_key");
    assert_eq!(row.try_get::<Option<i16>, _>("response_status")?, Some(426));
    Ok(())
}

#[tokio::test]
async fn responses_api_key_request_goes_to_the_openai_upstream() -> anyhow::Result<()> {
    let openai = sse_upstream("/v1/responses").await;
    let Some(mut h) = harness(&format!("{}/v1", openai.uri()), "http://127.0.0.1:1").await? else {
        return Ok(());
    };
    let unverified_provider_session = uuid::Uuid::new_v4();
    let request = Request::post("/codex/responses")
        .header("authorization", "Bearer sk-fx")
        .header("session-id", unverified_provider_session.to_string())
        .header("thread-id", unverified_provider_session.to_string())
        .body(Body::from(REQUEST))?;
    let response = routes(h.state.clone()).oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let client_bytes = response.into_body().collect().await?.to_bytes();
    assert_eq!(client_bytes.as_ref(), STREAM);
    let id = next_recorded(&mut h).await?;
    let row = sqlx::query(
        "SELECT auth_route, session_id, correlation FROM runtime.responses_exchanges WHERE id = $1",
    )
    .bind(id)
    .fetch_one(h.persist.pool())
    .await?;
    assert_eq!(row.try_get::<String, _>("auth_route")?, "api_key");
    assert!(row.try_get::<Option<i64>, _>("session_id")?.is_some());
    let correlation = row.try_get::<Value, _>("correlation")?;
    assert_eq!(
        correlation["normalized_session"]["source"],
        "proxy_thread_binding"
    );
    let invented: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sessions.sessions WHERE session_uuid=$1)")
            .bind(unverified_provider_session)
            .fetch_one(h.persist.pool())
            .await?;
    assert!(!invented);
    Ok(())
}

#[tokio::test]
async fn responses_unreachable_upstream_answers_502_and_records_the_error() -> anyhow::Result<()> {
    let Some(mut h) = harness("http://127.0.0.1:1", "http://127.0.0.1:1").await? else {
        return Ok(());
    };
    let request = Request::post("/codex/responses")
        .header("authorization", "Bearer sk-fx")
        .header("content-encoding", "zstd")
        .body(Body::from(&b"\x28\xb5\x2f\xfd-not-json"[..]))?;
    let response = routes(h.state.clone()).oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    let id = next_recorded(&mut h).await?;
    let row = sqlx::query(
        "SELECT status, error, request_body_encoding, model, request_body, evidence_status FROM runtime.responses_exchanges WHERE id = $1",
    )
    .bind(id)
    .fetch_one(h.persist.pool())
    .await?;
    assert_eq!(row.try_get::<String, _>("status")?, "error");
    assert!(
        row.try_get::<Option<String>, _>("error")?
            .is_some_and(|e| e.contains("upstream request failed"))
    );
    assert_eq!(
        row.try_get::<Option<String>, _>("request_body_encoding")?
            .as_deref(),
        Some("zstd")
    );
    assert_eq!(row.try_get::<Option<String>, _>("model")?, None);
    assert_eq!(
        row.try_get::<String, _>("evidence_status")?,
        "not_collected"
    );
    assert!(row.try_get::<Option<Vec<u8>>, _>("request_body")?.is_none());
    let capture = captured(&h, id).await?;
    assert_eq!(
        body(&h, capture.request_document_id).await?,
        b"\x28\xb5\x2f\xfd-not-json"
    );
    assert_eq!(capture.metadata["request_stored_encoding"], "zstd");
    assert!(capture.metadata["request_decoding_error"].is_string());
    Ok(())
}
