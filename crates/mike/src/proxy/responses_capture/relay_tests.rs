use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::Row;
use tokio::sync::mpsc;
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::relay::upstream_url;
use super::{ResponsesState, Upstreams, routes};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::TestDb;

const STREAM: &[u8] = b"event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_fx\",\"status\":\"in_progress\",\"model\":\"gpt-fx\"}}\n\nevent: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"item\":{\"id\":\"msg_fx\",\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"h\\u00e9llo\"}]}}\n\nevent: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_fx\",\"status\":\"completed\",\"model\":\"gpt-fx\",\"usage\":{\"input_tokens\":7,\"output_tokens\":2,\"total_tokens\":9}}}\n\n";

const REQUEST: &[u8] = b"{\"model\":\"gpt-fx\",\"stream\":true,\"input\":[{\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"say  hello\\n\"}]}]}";

struct Harness {
    state: ResponsesState,
    recorded: mpsc::UnboundedReceiver<Result<i64, String>>,
    persist: PersistHandle,
}

async fn harness(openai: &str, chatgpt: &str) -> anyhow::Result<Option<Harness>> {
    let Some(db) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let handle = db.acquire().await?;
    let persist = PersistHandle::connect(&handle.database_url).await?;
    let upstreams = Upstreams {
        openai: openai.to_string(),
        chatgpt: chatgpt.to_string(),
    };
    let mut state = ResponsesState::new(reqwest::Client::new(), upstreams, Some(persist.clone()));
    let (tx, rx) = mpsc::unbounded_channel();
    state.recorded = Some(tx);
    Ok(Some(Harness {
        state,
        recorded: rx,
        persist,
    }))
}

async fn next_recorded(h: &mut Harness) -> anyhow::Result<i64> {
    match h.recorded.recv().await {
        Some(Ok(id)) => Ok(id),
        Some(Err(error)) => anyhow::bail!("exchange not recorded: {error}"),
        None => anyhow::bail!("recorder channel closed"),
    }
}

fn turn_metadata(session: uuid::Uuid, cwd: &str) -> String {
    format!(
        "{{\"session_id\":\"{session}\",\"thread_id\":\"{session}\",\"turn_id\":\"turn-fx\",\"request_kind\":\"turn\",\"workspaces\":{{\"{cwd}\":{{\"has_changes\":false}}}}}}"
    )
}

async fn sse_upstream(route: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(route))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .insert_header("x-request-id", "req_fx")
                .set_body_bytes(STREAM),
        )
        .mount(&server)
        .await;
    server
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
    let request = Request::post("/codex/responses")
        .header("content-type", "application/json")
        .header("authorization", "Bearer secret-token-fx")
        .header("chatgpt-account-id", "acct-fx")
        .header("session-id", session.to_string())
        .header("thread-id", session.to_string())
        .header("x-codex-turn-metadata", turn_metadata(session, &cwd))
        .body(Body::from(REQUEST))?;
    let response = routes(h.state.clone()).oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let client_bytes = response.into_body().collect().await?.to_bytes();
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
    assert_eq!(row.try_get::<Vec<u8>, _>("request_body")?, REQUEST);
    assert_eq!(row.try_get::<Vec<u8>, _>("forwarded_body")?, REQUEST);
    assert_eq!(
        row.try_get::<Option<Vec<u8>>, _>("response_body")?
            .as_deref(),
        Some(STREAM)
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
    assert_eq!(items.as_array().map(Vec::len), Some(1));
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
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
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
    let request = Request::post("/codex/responses")
        .header("authorization", "Bearer sk-fx")
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
    assert_eq!(row.try_get::<Option<i64>, _>("session_id")?, None);
    let correlation = row.try_get::<Value, _>("correlation")?;
    assert!(correlation["session_id"].get("unknown").is_some());
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
        "SELECT status, error, request_body_encoding, model, request_body FROM runtime.responses_exchanges WHERE id = $1",
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
        row.try_get::<Vec<u8>, _>("request_body")?,
        b"\x28\xb5\x2f\xfd-not-json"
    );
    Ok(())
}
