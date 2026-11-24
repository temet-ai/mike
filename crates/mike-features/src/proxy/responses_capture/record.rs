use crate::store::persistence::runtime_repo::RuntimePersist as _;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use mike_codex::responses::ResponsesSummary;
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use super::request_body::SharedCapture;
use super::spool::{Manifest, Spool};
use super::{AuthRoute, Correlation, ResponsesState};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::runtime::capture::CaptureExchangeInput;
use crate::store::persistence::workspace::RegisterWorkspaceSession;

#[path = "capture_progress.rs"]
mod progress;
pub(super) use progress::{accepted, finish_delivery, pre_forward};

const REGISTER_EVERY: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExchangeStatus {
    Complete,
    Error,
    Aborted,
    Refused,
}

impl ExchangeStatus {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Error => "error",
            Self::Aborted => "aborted",
            Self::Refused => "refused",
        }
    }
}

#[derive(Debug)]
pub struct ExchangeRecord {
    pub correlation: Correlation,
    pub auth_route: AuthRoute,
    pub websocket_refused: bool,
    pub method: String,
    pub path: String,
    pub upstream_url: String,
    pub request_headers: Value,
    pub forwarded_headers: Value,
    pub request_body: SharedCapture,
    pub response_body: SharedCapture,
    pub request_body_encoding: Option<String>,
    pub model: Option<String>,
    pub response_status: Option<u16>,
    pub response_headers: Option<Value>,
    pub response_is_event_stream: bool,
    pub upstream_request_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub codex_home: Option<std::path::PathBuf>,
    pub launch_session: Option<Uuid>,
    pub spool: Arc<Spool>,
    pub(super) injection: Option<super::injection::PendingInjection>,
    pub(super) progress: Option<Manifest>,
}

#[derive(Debug, Clone)]
pub struct Finished {
    pub status: ExchangeStatus,
    pub summary: ResponsesSummary,
    pub error: Option<String>,
}

pub fn request_model(body: &[u8], encoding: Option<&str>) -> Option<String> {
    if encoding.is_some_and(|e| !e.eq_ignore_ascii_case("identity")) {
        return None;
    }
    let value: Value = serde_json::from_slice(body).ok()?;
    value.get("model")?.as_str().map(str::to_owned)
}

pub fn manifest(record: &ExchangeRecord, finished: Option<&Finished>) -> Manifest {
    let summary = finished.map(|finished| &finished.summary);
    let metadata = json!({
        "method":record.method, "path":record.path, "upstream_url":record.upstream_url,
        "auth_route":record.auth_route.as_db_str(),
        "transport":if record.websocket_refused { "websocket_refused" } else { "http" },
        "request_headers":record.request_headers, "forwarded_headers":record.forwarded_headers,
        "request_body_encoding":record.request_body_encoding, "model":record.model,
        "request_stored_encoding":record.request_body_encoding.as_deref().unwrap_or("identity"),
        "sent_stored_encoding":record.request_body_encoding.as_deref().unwrap_or("identity"),
        "response_body_encoding":response_encoding(record),
        "response_stored_encoding":response_encoding(record),
        "response_status":record.response_status, "response_headers":record.response_headers,
        "response_is_event_stream":record.response_is_event_stream,
        "upstream_request_id":record.upstream_request_id, "correlation":record.correlation.evidence,
        "provider_session_id":record.correlation.provider_session_id,
        "provider_turn_id":record.correlation.provider_turn_id,
        "response_id":summary.and_then(|value| value.response_id.as_ref()),
        "response_model":summary.and_then(|value| value.model.as_ref()),
        "response_state":summary.and_then(|value| value.state.as_ref()),
        "usage":summary.and_then(|value| value.usage.as_ref()),
        "stream_events":summary.map(|value| value.events),
        "projection_gaps":summary.map(|value| value.undecodable_frames + value.oversized_frames),
        "projection_truncated":summary.is_some_and(|value| value.oversized_frames > 0),
        "projection_undecodable_frames":summary.map(|value| value.undecodable_frames),
        "error":finished.and_then(|value| value.error.as_ref()),
    });
    let mut manifest = Manifest {
        identity: super::wire_observation::source(record),
        exchange: CaptureExchangeInput {
            id: record.spool.id,
            session_uuid: record.correlation.session_uuid,
            provider_thread_id: record
                .correlation
                .provider_thread_id
                .clone()
                .or_else(|| record.correlation.provider_session_id.clone()),
            created_at: record.created_at,
            metadata,
            request_document_id: 0,
            sent_document_id: 0,
            response_document_id: None,
            status: finished
                .map_or("pending", |value| value.status.as_db_str())
                .into(),
        },
        request_predecessor: None,
        response_predecessor: None,
        request_complete: record.request_body.complete.load(Ordering::Acquire),
        response_complete: record.response_body.complete.load(Ordering::Acquire),
        response_received: record.response_status.is_some(),
        sent_changed: false,
        sent_complete: false,
        request_persisted: false,
        sent_persisted: false,
        injected_messages: Vec::new(),
        upstream_accepted: false,
    };
    progress::restore(&mut manifest, record.progress.as_ref());
    manifest
}

pub(super) async fn prepare_identity(state: &ResponsesState, record: &mut ExchangeRecord) {
    let Some(persist) = &state.persist else {
        return;
    };
    if let Err(error) = super::wire_observation::resolve_identity(persist, record).await {
        record.correlation.session_uuid = None;
        record.correlation.evidence["identity_error"] = json!(error);
        tracing::warn!(error, capture = %record.spool.id, "Codex capture has no resolved session");
        return;
    }
    register_session(state, persist, &record.correlation).await;
    register_notification_target(
        persist,
        &super::wire_observation::source(record),
        record.correlation.session_uuid,
    )
    .await;
}

pub(super) async fn register_notification_target(
    persist: &PersistHandle,
    identity: &super::spool::IdentityInput,
    session: Option<Uuid>,
) {
    if let (Some(session), Some(thread), Some(home)) = (
        session,
        identity.thread.as_deref(),
        identity.home.as_deref(),
    ) {
        crate::runtime::codex_home::observe_local_codex(persist, session, thread, home.to_str())
            .await;
    }
}

pub fn spawn_finish(state: ResponsesState, mut record: ExchangeRecord, mut finished: Finished) {
    let tasks = state.tasks.clone();
    tasks.spawn(async move {
        let outcome = finish(&state, &mut record, &mut finished).await;
        if let Err(error) = &outcome {
            tracing::error!(error, capture = %record.spool.id,
                staging = %record.spool.directory.display(), "Codex capture awaits recovery");
            metrics::counter!("codex_capture_write_failed_total").increment(1);
        }
        if let Some(tx) = &state.recorded {
            let _ = tx.send(outcome);
        }
    });
}

async fn finish(
    state: &ResponsesState,
    record: &mut ExchangeRecord,
    finished: &mut Finished,
) -> Result<i64, String> {
    let accepted = record
        .progress
        .as_ref()
        .is_some_and(|progress| progress.upstream_accepted);
    super::injection::finish(state, record, accepted).await;
    let (request, response) = tokio::join!(record.request_body.wait(), record.response_body.wait());
    for result in [request, response] {
        if let Err(error) = result {
            finished.status = ExchangeStatus::Error;
            finished.error = Some(format!("capture staging failed: {error}"));
        }
    }
    record.model = read_model(record).await.or_else(|| record.model.clone());
    if record.correlation.session_uuid.is_none() {
        prepare_identity(state, record).await;
    }
    record.correlation.evidence["capture_id"] = json!(record.spool.id);
    let mut manifest = manifest(record, Some(finished));
    record.spool.write_manifest(&manifest).await?;
    let persist = state
        .persist
        .as_ref()
        .ok_or("persistence is disabled; captured files retained")?;
    let _slot = state
        .slots
        .clone()
        .acquire_owned()
        .await
        .map_err(|error| error.to_string())?;
    super::capture_write::persist(persist, &record.spool.directory, &mut manifest).await?;
    finish_delivery(persist, &manifest)
        .await
        .map_err(|error| error.to_string())?;
    let id = project(state, persist, record, finished).await;
    tokio::fs::remove_dir_all(&record.spool.directory)
        .await
        .map_err(|error| error.to_string())?;
    Ok(id)
}

async fn read_model(record: &ExchangeRecord) -> Option<String> {
    let file = tokio::fs::File::open(&record.request_body.path)
        .await
        .ok()?;
    let mut reader = file.take(mike_codex::responses::MAX_FRAME_BYTES as u64);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await.ok()?;
    request_model(&bytes, record.request_body_encoding.as_deref())
}

async fn project(
    state: &ResponsesState,
    persist: &PersistHandle,
    record: &ExchangeRecord,
    finished: &Finished,
) -> i64 {
    register_session(state, persist, &record.correlation).await;
    if let Err(error) = super::wire_observation::record(persist, record, finished).await {
        tracing::warn!(error, capture = %record.spool.id, "Codex capture projection failed");
    }
    project_diagnostic(persist, record, finished).await
}

async fn project_diagnostic(
    persist: &PersistHandle,
    record: &ExchangeRecord,
    finished: &Finished,
) -> i64 {
    let diagnostic = super::diagnostic::exchange(record, finished);
    let limits = crate::store::persistence::runtime::diagnostics::DiagnosticLimits {
        enabled: false,
        ..Default::default()
    };
    match persist
        .runtime()
        .record_responses_diagnostic(&diagnostic, &limits)
        .await
    {
        Ok(outcome) => outcome.exchange_id.unwrap_or(0),
        Err(error) => {
            tracing::warn!(%error, capture = %record.spool.id, "Codex diagnostic projection failed");
            0
        }
    }
}

async fn due_for_registration(state: &ResponsesState, session: Uuid) -> bool {
    let mut seen = state.registered.lock().await;
    let now = Instant::now();
    seen.retain(|_, at| now.duration_since(*at) < REGISTER_EVERY);
    if seen.contains_key(&session) {
        return false;
    }
    if seen.len() == 1024
        && let Some(oldest) = seen.iter().min_by_key(|(_, at)| **at).map(|(key, _)| *key)
    {
        seen.remove(&oldest);
    }
    seen.insert(session, now);
    true
}

pub(super) async fn register_session(
    state: &ResponsesState,
    persist: &PersistHandle,
    c: &Correlation,
) {
    let (Some(session_uuid), Some(cwd)) = (
        c.session_uuid,
        c.cwd.as_deref().filter(|cwd| cwd.len() <= 4096),
    ) else {
        return;
    };
    if !due_for_registration(state, session_uuid).await {
        return;
    }
    let input = RegisterWorkspaceSession {
        session_uuid,
        project_path: Some(cwd),
        cwd,
        worktree: None,
        registered_via: "passive",
        intent: None,
        role: None,
        name: None,
        instructions: None,
        pid: None,
    };
    if let Err(error) = persist.register_runtime_workspace_session(input).await {
        tracing::warn!(%error, session_id = %session_uuid, "Codex session not registered on the roster");
    }
}

fn response_encoding(record: &ExchangeRecord) -> &str {
    record
        .response_headers
        .as_ref()
        .and_then(Value::as_array)
        .and_then(|headers| {
            headers
                .iter()
                .find(|entry| entry.get(0).and_then(Value::as_str) == Some("content-encoding"))
        })
        .and_then(|entry| entry.get(1))
        .and_then(Value::as_str)
        .unwrap_or("identity")
}
