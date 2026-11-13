use crate::store::persistence::runtime_repo::RuntimePersist as _;
use std::time::{Duration, Instant};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use mike_codex::responses::ResponsesSummary;
use serde_json::Value;
use tokio::sync::OwnedSemaphorePermit;

use super::request_body::SharedCapture;
use super::{AuthRoute, Correlation, ResponsesState};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::workspace::RegisterWorkspaceSession;

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
    pub request_body_encoding: Option<String>,
    pub model: Option<String>,
    pub response_status: Option<u16>,
    pub response_headers: Option<Value>,
    pub response_is_event_stream: bool,
    pub upstream_request_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub diagnostics_enabled: bool,
    pub slot: Option<OwnedSemaphorePermit>,
    pub codex_home: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Finished {
    pub status: ExchangeStatus,
    pub response_body: Option<Bytes>,
    pub summary: ResponsesSummary,
    pub evidence_complete: bool,
    pub evidence_refused: bool,
    pub error: Option<String>,
}

pub fn request_model(body: &[u8], encoding: Option<&str>) -> Option<String> {
    if encoding.is_some_and(|e| !e.eq_ignore_ascii_case("identity")) {
        return None;
    }
    let value: Value = serde_json::from_slice(body).ok()?;
    value.get("model")?.as_str().map(str::to_owned)
}

pub fn spawn_finish(state: ResponsesState, mut record: ExchangeRecord, finished: Finished) {
    if record.slot.is_none() {
        metrics::counter!("codex_observation_budget_refused_total").increment(1);
        tracing::warn!(path = %record.path, "Codex wire observation unavailable: in-flight budget exhausted");
        return;
    }
    tokio::spawn(async move {
        let outcome = tokio::time::timeout(
            Duration::from_secs(5),
            finish(&state, &mut record, &finished),
        )
        .await
        .unwrap_or_else(|_| Err("Codex observation persistence deadline exceeded".into()));
        if let Err(error) = &outcome {
            tracing::warn!(error = %error, path = %record.path, "Codex Responses observation was not recorded");
            metrics::counter!("codex_observation_write_failed_total").increment(1);
        }
        if let Some(tx) = &state.recorded {
            let _ = tx.send(outcome);
        }
    });
}

async fn finish(
    state: &ResponsesState,
    record: &mut ExchangeRecord,
    finished: &Finished,
) -> Result<i64, String> {
    let persist = state.persist.as_ref().ok_or("persistence is disabled")?;
    super::wire_observation::resolve_identity(persist, record).await?;
    register_session(state, persist, &record.correlation).await;
    super::wire_observation::record(persist, record, finished).await?;
    let diagnostic = super::diagnostic::exchange(record, finished);
    let limits = crate::store::persistence::runtime::diagnostics::DiagnosticLimits {
        enabled: record.diagnostics_enabled,
        ..Default::default()
    };
    let outcome = persist
        .runtime()
        .record_responses_diagnostic(&diagnostic, &limits)
        .await
        .map_err(|e| e.to_string())?;
    outcome
        .exchange_id
        .ok_or_else(|| "diagnostic metadata budget refused; normalized observation retained".into())
}

fn due_for_registration(state: &ResponsesState, session: uuid::Uuid) -> bool {
    let Ok(mut seen) = state.registered.lock() else {
        return true;
    };
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

async fn register_session(state: &ResponsesState, persist: &PersistHandle, c: &Correlation) {
    let (Some(session_uuid), Some(cwd)) = (
        c.session_uuid,
        c.cwd.as_deref().filter(|cwd| cwd.len() <= 4096),
    ) else {
        return;
    };
    if !due_for_registration(state, session_uuid) {
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
        tracing::warn!(error = %error, session_id = %session_uuid, "Codex session not registered on the roster");
    }
}
