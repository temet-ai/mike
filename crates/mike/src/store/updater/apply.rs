use std::sync::{Arc, LazyLock};

use dashmap::DashSet;
use tokio::sync::RwLock;

use super::session_identity;
use crate::store::RequestStore;
use crate::store::models::{ProxyEvent, RequestId, RequestStatus};
use crate::types::{TokenUsage, ToolCall};

static ANALYSIS_COMPLETED: LazyLock<DashSet<RequestId>> = LazyLock::new(DashSet::new);

pub(crate) fn analysis_completed(id: &RequestId) -> bool {
    ANALYSIS_COMPLETED.contains(id)
}

pub(super) const MAX_TOOL_CALLS: usize = 1000;

fn enqueue_session_identity_reminders(s: &mut RequestStore, request_id: &RequestId) {
    let Some((session_id, reminders)) = s.get(request_id).map(|req| {
        (
            req.session_id.clone(),
            session_identity::detect_reminders(&req.tool_calls, &req.session_id),
        )
    }) else {
        return;
    };
    for reminder in reminders {
        s.enqueue_reminder(&session_id, reminder);
    }
}

pub(crate) async fn run_analysis_and_notify(
    store: &Arc<RwLock<RequestStore>>,
    request_id: RequestId,
    text_for_analysis: &str,
) {
    let (registry, event_tx) = {
        let s = store.read().await;
        let Some((reg, tx)) = s.analysis_wired() else {
            return;
        };
        (reg.clone(), tx.clone())
    };
    let text = text_for_analysis;

    let registry_snapshot = registry.load();
    let diagnostics = registry_snapshot.run(text);
    let count = diagnostics.len();

    for diag in &diagnostics {
        metrics::counter!(
            crate::metrics::COUNTER_DIAGNOSTICS_TOTAL,
            "category" => diag.category.as_str(),
            "analyzer" => diag.analyzer.clone(),
        )
        .increment(1);
    }

    {
        let mut s = store.write().await;
        if let Some(session_id) = s.get(&request_id).map(|req| req.session_id.clone()) {
            s.record_latest_findings(&session_id, request_id, &diagnostics);
        }
        s.update(&request_id, |req| {
            req.diagnostics = diagnostics;
        });
        ANALYSIS_COMPLETED.insert(request_id);
        enqueue_session_identity_reminders(&mut s, &request_id);
    }

    let _ = event_tx.send(ProxyEvent::DiagnosticReady {
        id: request_id,
        count,
        accumulated_text: text_for_analysis.to_owned(),
    });
}

pub(super) async fn apply_event(store: &Arc<RwLock<RequestStore>>, event: &ProxyEvent) -> bool {
    match event {
        ProxyEvent::TextDelta { .. }
        | ProxyEvent::ThinkingDelta { .. }
        | ProxyEvent::SignatureDelta { .. } => false,
        ProxyEvent::ToolUseStart {
            id,
            tool_id,
            tool_name,
        } => {
            apply_tool_use_start(store, id, tool_id, tool_name).await;
            false
        }
        ProxyEvent::ToolInputDelta { id, partial_json } => {
            apply_tool_input_delta(store, id, partial_json).await;
            false
        }
        ProxyEvent::ToolUseEnd { id } => {
            apply_tool_use_end(store, id).await;
            false
        }
        ProxyEvent::UsageReceived { id, usage } => {
            apply_usage_received(store, id, usage).await;
            false
        }
        ProxyEvent::MessageFieldsReceived { id, fields } => {
            let mut s = store.write().await;
            s.update(id, |req| {
                crate::extract::response::merge_message_fields(&mut req.message_fields, fields);
            });
            false
        }
        ProxyEvent::ResponseComplete {
            id,
            stop_reason,
            stop_details,
            usage,
        } => {
            apply_response_complete(store, id, stop_reason, stop_details, usage.as_ref()).await;
            true
        }
        ProxyEvent::ResponseError { id, error } => {
            apply_response_error(store, id, error).await;
            true
        }
        ProxyEvent::ModelConfirmed { id, model } => {
            apply_model_confirmed(store, id, model).await;
            false
        }
        ProxyEvent::UpstreamMessageIdReceived {
            id,
            upstream_message_id,
        } => {
            apply_upstream_message_id_received(store, id, upstream_message_id).await;
            false
        }
        ProxyEvent::DiagnosticReady { .. }
        | ProxyEvent::RowChanged { .. }
        | ProxyEvent::ResyncRequired { .. }
        | ProxyEvent::MemoryShed(_)
        | ProxyEvent::MemoryTick(_)
        | ProxyEvent::StreamingStarted { .. }
        | ProxyEvent::SteeringFired { .. }
        | ProxyEvent::UserPromptReady { .. }
        | ProxyEvent::OperatorTurnClosed { .. } => false,
    }
}

async fn apply_tool_use_start(
    store: &Arc<RwLock<RequestStore>>,
    id: &RequestId,
    tool_id: &str,
    tool_name: &str,
) {
    let mut s = store.write().await;
    s.update(id, |req| {
        if req.tool_calls.len() >= MAX_TOOL_CALLS {
            tracing::warn!(
                request_id = %req.id,
                max = MAX_TOOL_CALLS,
                "tool_calls limit reached, ignoring further tool calls"
            );
            return;
        }
        req.tool_calls.push(ToolCall {
            tool_id: tool_id.to_string(),
            name: tool_name.to_string(),
            input_json: String::new(),
            complete: false,
        });
    });
}

async fn apply_tool_input_delta(
    store: &Arc<RwLock<RequestStore>>,
    id: &RequestId,
    partial_json: &str,
) {
    let mut s = store.write().await;
    s.update(id, |req| {
        if let Some(last) = req.tool_calls.last_mut() {
            last.input_json.push_str(partial_json);
        }
    });
}

async fn apply_tool_use_end(store: &Arc<RwLock<RequestStore>>, id: &RequestId) {
    let mut s = store.write().await;
    s.update(id, |req| {
        if let Some(last) = req.tool_calls.last_mut() {
            last.complete = true;
        }
    });
}

async fn apply_usage_received(
    store: &Arc<RwLock<RequestStore>>,
    id: &RequestId,
    usage: &TokenUsage,
) {
    let mut s = store.write().await;
    s.update(id, |req| {
        if let Some(existing) = &mut req.token_usage {
            if existing.input_tokens == 0 {
                existing.input_tokens = usage.input_tokens;
            }
            if existing.cache_creation_input_tokens.is_none() {
                existing.cache_creation_input_tokens = usage.cache_creation_input_tokens;
            }
            if existing.cache_read_input_tokens.is_none() {
                existing.cache_read_input_tokens = usage.cache_read_input_tokens;
            }
        } else {
            req.token_usage = Some(usage.clone());
        }
    });
}

async fn apply_response_complete(
    store: &Arc<RwLock<RequestStore>>,
    id: &RequestId,
    stop_reason: &Option<String>,
    stop_details: &Option<serde_json::Value>,
    usage: Option<&TokenUsage>,
) {
    let mut s = store.write().await;
    s.update(id, |req| {
        req.status = RequestStatus::Complete;
        req.stop_reason.clone_from(stop_reason);
        req.stop_details.clone_from(stop_details);
        if let Some(u) = usage {
            if let Some(existing) = &mut req.token_usage {
                existing.output_tokens = u.output_tokens;
            } else {
                req.token_usage = Some(u.clone());
            }
        }
        req.completed_at = Some(chrono::Utc::now());
    });
}

async fn apply_response_error(store: &Arc<RwLock<RequestStore>>, id: &RequestId, error: &str) {
    let mut s = store.write().await;
    s.update(id, |req| {
        req.status = RequestStatus::Error(error.to_string());
        req.completed_at = Some(chrono::Utc::now());
    });
}

async fn apply_model_confirmed(store: &Arc<RwLock<RequestStore>>, id: &RequestId, model: &str) {
    if model.is_empty() {
        return;
    }
    let mut s = store.write().await;
    s.update(id, |req| {
        req.model = Some(model.to_string());
    });
}

async fn apply_upstream_message_id_received(
    store: &Arc<RwLock<RequestStore>>,
    id: &RequestId,
    upstream_message_id: &str,
) {
    if upstream_message_id.is_empty() {
        return;
    }
    let mut s = store.write().await;
    s.update(id, |req| {
        req.upstream_message_id = Some(upstream_message_id.to_string());
    });
}

#[cfg(test)]
mod tests;
