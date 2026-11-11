use std::time::{Duration, Instant};

use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde_json::Value;

use super::{AuthRoute, Correlation, ResponsesState};
use crate::sse::parser::responses::{ResponsesSummary, summarize_json, summarize_stream};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::workspace::RegisterWorkspaceSession;

const REGISTER_EVERY: Duration = Duration::from_secs(60);

const INSERT_EXCHANGE: &str = "INSERT INTO runtime.responses_exchanges
     (session_id, provider_session_id, provider_thread_id, provider_turn_id,
      request_kind, correlation, auth_route, transport, method, path, upstream_url,
      request_headers, forwarded_headers, request_body, forwarded_body,
      request_body_encoding, model, response_status, response_headers, response_body,
      upstream_request_id, response_id, response_model, response_state, usage,
      output_items, stream_events, error, status, created_at, completed_at)
 VALUES ((SELECT id FROM sessions.sessions WHERE session_uuid = $1),
         $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17,
         $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, now())
 RETURNING id";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExchangeStatus {
    Complete,
    Error,
    Aborted,
    Refused,
}

impl ExchangeStatus {
    fn as_db_str(self) -> &'static str {
        match self {
            ExchangeStatus::Complete => "complete",
            ExchangeStatus::Error => "error",
            ExchangeStatus::Aborted => "aborted",
            ExchangeStatus::Refused => "refused",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ExchangeRecord {
    pub(crate) correlation: Correlation,
    pub(crate) auth_route: AuthRoute,
    pub(crate) websocket_refused: bool,
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) upstream_url: String,
    pub(crate) request_headers: Value,
    pub(crate) forwarded_headers: Value,
    pub(crate) request_body: Bytes,
    pub(crate) forwarded_body: Bytes,
    pub(crate) request_body_encoding: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) response_status: Option<u16>,
    pub(crate) response_headers: Option<Value>,
    pub(crate) response_is_event_stream: bool,
    pub(crate) upstream_request_id: Option<String>,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct Finished {
    pub(crate) status: ExchangeStatus,
    pub(crate) response_body: Option<Bytes>,
    pub(crate) error: Option<String>,
}

pub(super) fn request_model(body: &[u8], encoding: Option<&str>) -> Option<String> {
    if encoding.is_some_and(|e| !e.eq_ignore_ascii_case("identity")) {
        return None;
    }
    let value: Value = serde_json::from_slice(body).ok()?;
    value.get("model")?.as_str().map(str::to_string)
}

fn summary_of(record: &ExchangeRecord, finished: &Finished) -> ResponsesSummary {
    match &finished.response_body {
        Some(body) if record.response_is_event_stream => summarize_stream(body),
        Some(body) => summarize_json(body),
        None => ResponsesSummary::default(),
    }
}

pub(super) fn spawn_finish(state: ResponsesState, record: ExchangeRecord, finished: Finished) {
    tokio::spawn(async move {
        let outcome = finish(&state, &record, &finished).await;
        if let Err(error) = &outcome {
            tracing::warn!(error = %error, path = %record.path, "Codex Responses exchange was not recorded");
        }
        if let Some(tx) = &state.recorded {
            let _ = tx.send(outcome);
        }
    });
}

async fn finish(
    state: &ResponsesState,
    record: &ExchangeRecord,
    finished: &Finished,
) -> Result<i64, String> {
    let Some(persist) = &state.persist else {
        return Err("persistence is disabled".to_string());
    };
    register_session(state, persist, &record.correlation).await;
    let summary = summary_of(record, finished);
    insert(persist, record, finished, &summary)
        .await
        .map_err(|e| e.to_string())
}

fn due_for_registration(state: &ResponsesState, session: uuid::Uuid) -> bool {
    let Ok(mut seen) = state.registered.lock() else {
        return true;
    };
    let now = Instant::now();
    match seen.get(&session) {
        Some(at) if now.duration_since(*at) < REGISTER_EVERY => false,
        _ => {
            seen.insert(session, now);
            true
        }
    }
}

async fn register_session(state: &ResponsesState, persist: &PersistHandle, c: &Correlation) {
    let (Some(session_uuid), Some(cwd)) = (c.session_uuid, c.cwd.as_deref()) else {
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
    if let Err(error) = persist.register_workspace_session(input).await {
        tracing::warn!(error = %error, session_id = %session_uuid, "Codex session not registered on the roster");
    }
}

async fn insert(
    persist: &PersistHandle,
    record: &ExchangeRecord,
    finished: &Finished,
    summary: &ResponsesSummary,
) -> Result<i64, sqlx::Error> {
    let c = &record.correlation;
    let transport = if record.websocket_refused {
        "websocket_refused"
    } else {
        "http"
    };
    let output_items = Value::Array(summary.output_items.clone());
    let response_body = finished.response_body.as_ref().map(|b| b.to_vec());
    let stream_events = record
        .response_is_event_stream
        .then(|| i32::try_from(summary.events).unwrap_or(i32::MAX));
    let error = finished.error.clone().or_else(|| summary.error.clone());
    sqlx::query_scalar::<_, i64>(INSERT_EXCHANGE)
        .bind(c.session_uuid)
        .bind(&c.provider_session_id)
        .bind(&c.provider_thread_id)
        .bind(&c.provider_turn_id)
        .bind(&c.request_kind)
        .bind(&c.evidence)
        .bind(record.auth_route.as_db_str())
        .bind(transport)
        .bind(&record.method)
        .bind(&record.path)
        .bind(&record.upstream_url)
        .bind(&record.request_headers)
        .bind(&record.forwarded_headers)
        .bind(record.request_body.as_ref())
        .bind(record.forwarded_body.as_ref())
        .bind(&record.request_body_encoding)
        .bind(&record.model)
        .bind(
            record
                .response_status
                .map(|s| i16::try_from(s).unwrap_or(i16::MAX)),
        )
        .bind(&record.response_headers)
        .bind(response_body)
        .bind(&record.upstream_request_id)
        .bind(&summary.response_id)
        .bind(&summary.model)
        .bind(&summary.state)
        .bind(&summary.usage)
        .bind(output_items)
        .bind(stream_events)
        .bind(error)
        .bind(finished.status.as_db_str())
        .bind(record.created_at)
        .fetch_one(persist.pool())
        .await
}
