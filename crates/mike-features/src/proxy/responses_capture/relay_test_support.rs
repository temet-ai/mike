use super::{ResponsesState, Upstreams};
use crate::store::persistence::runtime_repo::RuntimePersist as _;
use crate::store::persistence::{PersistHandle, test_support::TestDb};
use tokio::sync::mpsc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub(super) const STREAM: &[u8] = b"event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_fx\",\"status\":\"in_progress\",\"model\":\"gpt-fx\"}}\n\nevent: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"item\":{\"id\":\"msg_fx\",\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"h\\u00e9llo\"}]}}\n\nevent: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_fx\",\"status\":\"completed\",\"model\":\"gpt-fx\",\"usage\":{\"input_tokens\":7,\"output_tokens\":2,\"total_tokens\":9}}}\n\n";

pub(super) const REQUEST: &[u8] = b"{\"model\":\"gpt-fx\",\"stream\":true,\"input\":[{\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"say  hello\\n\"}]}]}";

pub(super) struct Harness {
    pub(super) home: tempfile::TempDir,
    pub(super) state: ResponsesState,
    pub(super) recorded: mpsc::UnboundedReceiver<Result<i64, String>>,
    pub(super) persist: PersistHandle,
}

pub(super) async fn harness(openai: &str, chatgpt: &str) -> anyhow::Result<Option<Harness>> {
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
    let home = tempfile::tempdir()?;
    state.capture_root = Some(std::sync::Arc::new(home.path().join("capture")));
    let (tx, rx) = mpsc::unbounded_channel();
    state.recorded = Some(tx);
    Ok(Some(Harness {
        home,
        state,
        recorded: rx,
        persist,
    }))
}

pub(super) async fn next_recorded(h: &mut Harness) -> anyhow::Result<i64> {
    match tokio::time::timeout(std::time::Duration::from_secs(10), h.recorded.recv()).await? {
        Some(Ok(id)) => Ok(id),
        Some(Err(error)) => anyhow::bail!("exchange not recorded: {error}"),
        None => anyhow::bail!("recorder channel closed"),
    }
}

pub(super) async fn bind_session(h: &Harness, session: uuid::Uuid) -> anyhow::Result<()> {
    let namespace = crate::launch::codex::namespace_for(h.home.path());
    h.persist.ensure_runtime_session(session).await?;
    h.persist
        .runtime()
        .register_runtime_namespace(&namespace)
        .await?;
    h.persist
        .runtime()
        .bind_runtime_thread(&crate::runtime::RuntimeThreadBinding {
            mike_session_id: session,
            provider: crate::runtime::ProviderThreadRef {
                namespace_id: namespace.id,
                thread_id: session.to_string(),
            },
            provider_session_id: Some(session.to_string()),
            owning_seat_id: None,
            native_parent: None,
            forked_from: None,
        })
        .await?;
    Ok(())
}

pub(super) fn turn_metadata(session: uuid::Uuid, cwd: &str) -> String {
    format!(
        "{{\"session_id\":\"{session}\",\"thread_id\":\"{session}\",\"turn_id\":\"turn-fx\",\"request_kind\":\"turn\",\"workspaces\":{{\"{cwd}\":{{\"has_changes\":false}}}}}}"
    )
}

pub(super) async fn sse_upstream(route: &str) -> MockServer {
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

pub(super) async fn captured(
    h: &Harness,
    diagnostic: i64,
) -> anyhow::Result<crate::store::persistence::runtime::capture::CaptureExchangeRow> {
    let correlation: serde_json::Value =
        sqlx::query_scalar("SELECT correlation FROM runtime.responses_exchanges WHERE id=$1")
            .bind(diagnostic)
            .fetch_one(h.persist.pool())
            .await?;
    let id = correlation
        .get("capture_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("diagnostic has no full capture reference"))?;
    h.persist
        .runtime()
        .get_capture_exchange(uuid::Uuid::parse_str(id)?)
        .await?
        .ok_or_else(|| anyhow::anyhow!("full exchange missing"))
}

pub(super) async fn body(h: &Harness, document: i64) -> anyhow::Result<Vec<u8>> {
    let mut body = Vec::new();
    let mut offset = 0;
    loop {
        let rows = h
            .persist
            .runtime()
            .capture_document_chunks(document, offset, 100)
            .await?;
        if rows.is_empty() {
            break;
        }
        offset += rows.len() as i64;
        for row in rows {
            body.extend(row.body);
        }
    }
    Ok(body)
}
