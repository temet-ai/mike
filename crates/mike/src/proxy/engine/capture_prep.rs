use axum::http::request::Parts;
use bytes::Bytes;
use serde_json::Value;

use crate::capture;
use crate::compress::manifest::{ManifestBuilder, sha256_hex};
use crate::config::AppState;
use crate::extract::request::{TurnOrigin, TurnType};
use crate::extract::signals::RequestSignals;
use crate::store::models::{InterceptedRequest, RequestId, SystemPromptObservation, ToolResult};

pub(super) struct CapturePrepInputs<'a> {
    pub(super) state: &'a AppState,
    pub(super) parts: &'a Parts,
    pub(super) body_bytes: &'a Bytes,
    pub(super) upstream_body: &'a Bytes,
    pub(super) request_id: RequestId,
    pub(super) signals: RequestSignals,
    pub(super) stored_body: Option<Bytes>,
    pub(super) json_body: Option<&'a Value>,
    pub(super) observed_prompt_sha256: Option<String>,
    pub(super) observed_composition_id: Option<i64>,
    pub(super) observed_composition_applied: bool,
    pub(super) observed_unknown_blocks: Option<Value>,
    pub(super) filter_applied: bool,
    pub(super) firing_rule_ids: Vec<i64>,
    pub(super) manifest: ManifestBuilder,
}

pub(super) struct CapturePrepOutcome {
    pub(super) intercepted: InterceptedRequest,
    pub(super) streaming_model: Option<String>,
    pub(super) body_tool_results: Vec<ToolResult>,
    pub(super) human_prompt_for_classification: String,
    pub(super) is_new_session: bool,
    pub(super) session_id: String,
    pub(super) project_path: String,
    pub(super) turn_origin: TurnOrigin,
    pub(super) turn_type: TurnType,
}

pub(super) async fn prepare_capture(inputs: CapturePrepInputs<'_>) -> CapturePrepOutcome {
    let system_prompt_obs = build_request_system_prompt_observation(&inputs);

    let CapturePrepInputs {
        state,
        parts,
        body_bytes,
        upstream_body,
        request_id,
        signals,
        stored_body,
        json_body,
        manifest,
        ..
    } = inputs;
    log_proxying_request(&signals, request_id, state.in_flight.len());

    let RequestSignals {
        session_id,
        model,
        is_streaming,
        message_count,
        is_tool_loop,
        last_user_message,
        tool_names,
        params,
        human_prompt,
        turn_type: turn_type_enum,
        turn_origin: turn_origin_enum,
        subagent_persona,
        request_headers: masked_headers,
        claude_code_agent_id,
        project_path: lesson_project_path,
        ..
    } = signals;
    let turn_type = turn_type_enum.to_string();
    let turn_origin = turn_origin_enum.to_string();

    let body_tool_results = capture::extract_body_tool_results(json_body);

    let streaming_model = model.clone();

    let (resolved_project, is_new_session) = resolve_project(state, &session_id).await;

    let human_prompt_for_classification = human_prompt.clone();

    let sent_differs = upstream_body.as_ref() != body_bytes.as_ref();
    let sent_body_for_store = sent_differs.then(|| upstream_body.clone());

    let transformation_manifest_value =
        close_transformation_manifest(manifest, upstream_body, request_id, &session_id);

    let intercepted = capture::build_intercepted_request(capture::InterceptedRequestInputs {
        id: request_id,
        session_id: session_id.clone(),
        method: parts.method.to_string(),
        path: parts.uri.path().to_string(),
        request_headers: masked_headers,
        request_body: stored_body,
        model,
        is_streaming,
        message_count,
        is_tool_loop,
        last_user_message,
        tool_names,
        params,
        project: resolved_project,
        git_repo: None,
        human_prompt,
        turn_type,
        turn_origin,
        subagent_persona,
        tool_results_in_body: body_tool_results.clone(),
        filter_applied: sent_differs,
        sent_body: sent_body_for_store,
        transformation_manifest: transformation_manifest_value,
        system_prompt: system_prompt_obs,
        claude_code_agent_id,
    });

    CapturePrepOutcome {
        intercepted,
        streaming_model,
        body_tool_results,
        human_prompt_for_classification,
        is_new_session,
        session_id,
        project_path: lesson_project_path,
        turn_origin: turn_origin_enum,
        turn_type: turn_type_enum,
    }
}

fn build_request_system_prompt_observation(
    inputs: &CapturePrepInputs<'_>,
) -> SystemPromptObservation {
    let system_blocks_value = extract_system_blocks(inputs.body_bytes);
    capture::build_system_prompt_observation(capture::SystemPromptObservationInputs {
        prompt_sha256: inputs.observed_prompt_sha256.clone(),
        system_blocks_value,
        composition_id: inputs.observed_composition_id,
        composition_applied: inputs.observed_composition_applied,
        unknown_blocks: inputs.observed_unknown_blocks.clone(),
        filter_modified: inputs.filter_applied,
        firing_rule_ids: inputs.firing_rule_ids.clone(),
    })
}

fn log_proxying_request(signals: &RequestSignals, request_id: RequestId, in_flight: usize) {
    tracing::info!(
        request_id = %request_id,
        session_id = %signals.session_id,
        model = ?signals.model,
        messages = signals.message_count,
        stream = signals.is_streaming,
        tool_loop = signals.is_tool_loop,
        tools = signals.tool_names.len(),
        in_flight,
        "proxying request"
    );
}

fn extract_system_blocks(body_bytes: &Bytes) -> Option<Value> {
    serde_json::from_slice::<Value>(body_bytes)
        .ok()
        .and_then(|v| v.get("system").cloned())
        .filter(Value::is_array)
}

async fn resolve_project(state: &AppState, session_id: &str) -> (Option<String>, bool) {
    let s = state.store.read().await;
    let adopted = s.project_for_session(session_id);
    let is_new = s.session_is_new(session_id);
    (adopted, is_new)
}

fn close_transformation_manifest(
    manifest: ManifestBuilder,
    upstream_body: &Bytes,
    request_id: RequestId,
    session_id: &str,
) -> Option<Value> {
    let manifest = manifest.finish(sha256_hex(upstream_body.as_ref()));
    if let Err(e) = manifest.verify_chain() {
        log_manifest_chain_did_not_close(request_id, session_id, &e);
    }
    match serde_json::to_value(&manifest) {
        Ok(v) => Some(v),
        Err(e) => {
            log_manifest_serialize_failed(request_id, session_id, &e);
            None
        }
    }
}

fn log_manifest_chain_did_not_close(
    request_id: RequestId,
    session_id: &str,
    e: &crate::compress::manifest::ChainError,
) {
    tracing::warn!(
        request_id = %request_id,
        session_id = %session_id,
        error = %e,
        "transformation manifest hash chain did not close"
    );
}

fn log_manifest_serialize_failed(request_id: RequestId, session_id: &str, e: &serde_json::Error) {
    tracing::warn!(
        request_id = %request_id,
        session_id = %session_id,
        error = %e,
        "failed to serialize transformation manifest"
    );
}
