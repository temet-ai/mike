use std::sync::Arc;

use axum::http::request::Parts;
use bytes::Bytes;
use serde_json::Value;

use crate::compose_prompt;
use crate::config::AppState;
use crate::error::ProxyError;
use crate::feature_toggles::ResolvedToggles;
use crate::store::models::RequestId;

use super::capture_register::{self, CaptureAndRegisterArgs, CaptureAndRegisterOutcome};
use super::guards::{self, ComposeAndGuardArgs, ComposeAndGuardOutcome};
use super::mutation_stages::{self, MutationOutcome, MutationStageParams};
use super::{AcceptedRequest, PipelineOutcome, PipelineResult};

pub(crate) async fn run_request_pipeline(
    state: &AppState,
    accepted: AcceptedRequest,
) -> Result<PipelineOutcome, ProxyError> {
    let AcceptedRequest {
        parts,
        body_bytes,
        signals,
        session_toggles,
        stored_body,
    } = accepted;

    let json_body: Option<Value> = serde_json::from_slice(&body_bytes).ok();

    let request_id = RequestId::next();

    emit_header_capture_shed_if_needed(
        state,
        request_id,
        &signals.session_id,
        signals.dropped_header_count,
        signals.dropped_header_bytes,
    );

    let MutationOutcome {
        post_filter_body,
        filter_applied,
        firing_rule_ids,
        injected_lesson_ids,
        mut manifest,
        injected_reminders,
        proxy_tools_declared,
        context_focus_asked,
    } = mutation_stages::run_mutation_stages(MutationStageParams {
        state,
        body_bytes: &body_bytes,
        session_toggles: &session_toggles,
        signals: &signals,
        request_id,
    })
    .await;

    let is_count_tokens_probe = parts.uri.path().ends_with("/count_tokens");
    let compose_result = match guards::compose_and_guard(ComposeAndGuardArgs {
        state,
        post_filter_body,
        body_bytes: &body_bytes,
        signals: &signals,
        is_count_tokens_probe,
        request_id,
        manifest: &mut manifest,
        filter_applied,
        json_body: json_body.as_ref(),
        parts: &parts,
    })
    .await?
    {
        ComposeAndGuardOutcome::EarlyResponse(response) => {
            return Ok(PipelineOutcome::EarlyResponse(response));
        }
        ComposeAndGuardOutcome::Continue(result) => result,
    };
    let compose_prompt::stage::ComposeResult {
        upstream_body,
        observed_prompt_sha256,
        observed_composition_id,
        observed_composition_applied,
        observed_unknown_blocks,
        composed_system_text,
    } = compose_result;

    let carrier_eligible = is_primary_lane(&signals.turn_origin);
    let tail = match capture_register::capture_and_register(CaptureAndRegisterArgs {
        state,
        parts: &parts,
        body_bytes: &body_bytes,
        upstream_body: &upstream_body,
        request_id,
        signals,
        stored_body,
        json_body,
        observed_prompt_sha256,
        observed_composition_id,
        observed_composition_applied,
        observed_unknown_blocks,
        filter_applied,
        firing_rule_ids,
        manifest,
        injected_lesson_ids,
        injected_reminders,
    })
    .await?
    {
        CaptureAndRegisterOutcome::EarlyResponse(response) => {
            return Ok(PipelineOutcome::EarlyResponse(response));
        }
        CaptureAndRegisterOutcome::Registered(tail) => tail,
    };

    Ok(continue_with(
        (parts, upstream_body),
        session_toggles,
        tail,
        (observed_composition_applied, composed_system_text),
        (carrier_eligible, proxy_tools_declared, context_focus_asked),
    ))
}

fn continue_with(
    (parts, upstream_body): (Parts, Bytes),
    session_toggles: Arc<ResolvedToggles>,
    tail: capture_register::CapturedPipelineTail,
    (observed_composition_applied, composed_system_text): (bool, Option<String>),
    (carrier_eligible, proxy_tools_declared, context_focus_asked): (bool, bool, bool),
) -> PipelineOutcome {
    PipelineOutcome::Continue(Box::new(PipelineResult {
        parts,
        upstream_body,
        request_id: tail.request_id,
        session_id: tail.session_id,
        session_toggles,
        in_flight_guard: tail.in_flight_guard,
        streaming_model: tail.streaming_model,
        injected_reminders: tail.injected_reminders,
        observed_composition_applied,
        composed_system_text,
        project_path: tail.project_path,
        carrier_eligible,
        proxy_tools_declared,
        context_focus_asked,
    }))
}

fn emit_header_capture_shed_if_needed(
    state: &AppState,
    request_id: RequestId,
    session_id: &str,
    dropped_header_count: usize,
    dropped_header_bytes: usize,
) {
    if dropped_header_count == 0 && dropped_header_bytes == 0 {
        return;
    }
    crate::observability::shed::emit(
        crate::observability::shed::MemoryShedEvent {
            event: "memory.shed",
            path: crate::observability::shed::ShedPath::HeaderCapture,
            reason: crate::observability::shed::ShedReason::RejectIncoming,
            bytes: dropped_header_bytes as u64,
            limit_bytes: crate::proxy::handler::MAX_HEADER_BYTES as u64,
            request_id: Some(request_id.as_u64()),
            session_id: Some(session_id.to_string()),
            note: "header_capture_clipped",
        },
        Some(&state.event_tx),
    );
}

fn is_primary_lane(turn_origin: &crate::extract::request::TurnOrigin) -> bool {
    matches!(
        turn_origin,
        crate::extract::request::TurnOrigin::Primary
            | crate::extract::request::TurnOrigin::PrimaryFirst
    )
}
