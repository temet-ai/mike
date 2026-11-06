#[cfg(test)]
mod tests;

use std::sync::Arc;

use bytes::Bytes;
use serde_json::Value;
use tokio::sync::{RwLock, broadcast};

use crate::in_flight::{CapacityError, InFlightGuard, InFlightRecord, InFlightRegistry};
use crate::store::models::{
    InterceptedRequest, ProxyEvent, RequestId, RequestStatus, SystemPromptObservation,
    SystemPromptSource, ToolResult,
};
use crate::store::{AttachToolResultsOutcome, RequestStore};
use crate::types::RequestParams;

pub(crate) struct InterceptedRequestInputs {
    pub(crate) id: RequestId,
    pub(crate) session_id: String,
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) request_headers: Vec<(String, String)>,
    pub(crate) request_body: Option<Bytes>,
    pub(crate) model: Option<String>,
    pub(crate) is_streaming: bool,
    pub(crate) message_count: usize,
    pub(crate) is_tool_loop: bool,
    pub(crate) last_user_message: String,
    pub(crate) tool_names: Vec<String>,
    pub(crate) params: RequestParams,
    pub(crate) project: Option<String>,
    pub(crate) git_repo: Option<String>,
    pub(crate) human_prompt: String,
    pub(crate) turn_type: String,
    pub(crate) turn_origin: String,
    pub(crate) subagent_persona: Option<String>,
    pub(crate) tool_results_in_body: Vec<ToolResult>,
    pub(crate) filter_applied: bool,
    pub(crate) sent_body: Option<Bytes>,
    pub(crate) transformation_manifest: Option<Value>,
    pub(crate) system_prompt: SystemPromptObservation,
    pub(crate) claude_code_agent_id: Option<String>,
}

pub(crate) fn build_intercepted_request(inputs: InterceptedRequestInputs) -> InterceptedRequest {
    InterceptedRequest {
        id: inputs.id,
        upstream_request_id: None,
        upstream_message_id: None,
        session_id: inputs.session_id,
        method: inputs.method,
        path: inputs.path,
        request_headers: inputs.request_headers,
        request_body: inputs.request_body,
        model: inputs.model,
        is_streaming: inputs.is_streaming,
        message_count: inputs.message_count,
        is_tool_loop: inputs.is_tool_loop,
        last_user_message: inputs.last_user_message,
        tool_names: inputs.tool_names,
        params: inputs.params,
        project: inputs.project,
        git_repo: inputs.git_repo,
        human_prompt: inputs.human_prompt,
        turn_type: inputs.turn_type,
        turn_origin: inputs.turn_origin,
        subagent_persona: inputs.subagent_persona,
        status: RequestStatus::Pending,
        response_status: None,
        response_headers: None,
        response_body: None,
        tool_calls: Vec::new(),
        token_usage: None,
        stop_reason: None,
        stop_details: None,
        message_fields: None,
        tool_results: Vec::new(),
        tool_results_in_body: inputs.tool_results_in_body,
        failed_stream_blocks: Vec::new(),
        expected_stream_block_count: 0,
        created_at: chrono::Utc::now(),
        completed_at: None,
        diagnostics: Vec::new(),
        filter_applied: inputs.filter_applied,
        filtered_request_body: None,
        sent_body: inputs.sent_body,
        transformation_manifest: inputs.transformation_manifest,
        system_prompt: inputs.system_prompt,
        intercepted: false,
        correction_request_id: None,
        agent_instance_id: None,
        claude_code_agent_id: inputs.claude_code_agent_id,
    }
}

pub(crate) struct SystemPromptObservationInputs {
    pub(crate) prompt_sha256: Option<String>,
    pub(crate) system_blocks_value: Option<Value>,
    pub(crate) composition_id: Option<i64>,
    pub(crate) composition_applied: bool,
    pub(crate) unknown_blocks: Option<Value>,
    pub(crate) filter_modified: bool,
    pub(crate) firing_rule_ids: Vec<i64>,
}

pub(crate) fn build_system_prompt_observation(
    inputs: SystemPromptObservationInputs,
) -> SystemPromptObservation {
    let blocks = if inputs.unknown_blocks.is_some() {
        None
    } else {
        inputs.system_blocks_value
    };
    let source = if blocks.is_some() {
        SystemPromptSource::Exact
    } else {
        SystemPromptSource::NotCaptured
    };
    SystemPromptObservation {
        prompt_sha256: inputs.prompt_sha256,
        blocks,
        composition_id: inputs.composition_id,
        composition_applied: inputs.composition_applied,
        unknown_blocks: inputs.unknown_blocks,
        filter_modified: inputs.filter_modified,
        firing_rule_ids: inputs.firing_rule_ids,
        source,
    }
}

pub(crate) fn extract_body_tool_results(json_body: Option<&Value>) -> Vec<ToolResult> {
    json_body
        .map(crate::store::updater::extract_tool_results_from_body)
        .unwrap_or_default()
}

pub(crate) async fn insert_stub_and_mint_id(
    store: &Arc<RwLock<RequestStore>>,
    intercepted: &mut InterceptedRequest,
    is_new_session: bool,
) -> RequestId {
    let request_id =
        RequestStore::insert_request_stub_to_db(store, intercepted, is_new_session).await;
    intercepted.id = request_id;
    request_id
}

pub(crate) async fn insert_and_attach_tool_results(
    store: &Arc<RwLock<RequestStore>>,
    intercepted: InterceptedRequest,
    session_id: &str,
    body_tool_results: Vec<ToolResult>,
) -> AttachToolResultsOutcome {
    let mut store = store.write().await;
    store.insert(intercepted);
    store.attach_tool_results(session_id, body_tool_results)
}

pub(crate) enum InFlightRegistration {
    Registered(InFlightGuard),
    CapacityExceeded { max: usize },
}

pub(crate) fn register_in_flight(
    in_flight: &Arc<InFlightRegistry>,
    store: &Arc<RwLock<RequestStore>>,
    event_tx: &broadcast::Sender<ProxyEvent>,
    request_id: RequestId,
    session_id: &str,
) -> InFlightRegistration {
    if let Err(cap) = in_flight.register(
        request_id,
        InFlightRecord {
            session_id: session_id.to_string(),
            started_at: std::time::Instant::now(),
            event_tx: event_tx.clone(),
        },
    ) {
        warn_in_flight_saturated(request_id, session_id, &cap);
        return InFlightRegistration::CapacityExceeded { max: cap.max };
    }
    let guard = InFlightGuard::new(in_flight.clone(), store.clone(), request_id);
    debug_log_in_flight_registered(in_flight, request_id, event_tx);
    InFlightRegistration::Registered(guard)
}

fn warn_in_flight_saturated(request_id: RequestId, session_id: &str, cap: &CapacityError) {
    tracing::warn!(
        request_id = %request_id,
        session_id = %session_id,
        current = cap.current,
        max = cap.max,
        "in-flight registry saturated; returning 503"
    );
}

fn debug_log_in_flight_registered(
    in_flight: &Arc<InFlightRegistry>,
    request_id: RequestId,
    event_tx: &broadcast::Sender<ProxyEvent>,
) {
    let Some(record) = in_flight.get(request_id) else {
        return;
    };
    debug_assert!(record.event_tx().same_channel(event_tx));
    tracing::debug!(
        request_id = %request_id,
        in_flight_session = record.session_id(),
        in_flight_age_ms = record.elapsed().as_millis() as u64,
        "in-flight registry entry created"
    );
}
