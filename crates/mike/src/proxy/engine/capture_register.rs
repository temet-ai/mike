use serde_json::Value;

use crate::capture;
use crate::config::AppState;
use crate::error::ProxyError;
use crate::extract::request::{TurnOrigin, TurnType};
use crate::extract::signals::RequestSignals;
use crate::in_flight::InFlightGuard;
use crate::store::AttachToolResultsOutcome;
use crate::store::models::{InterceptedRequest, ProxyEvent, RequestId, SystemReminder, ToolResult};
use crate::store::persistence::capture_classification::classify_author_kind;

use super::attribution;
use super::capture_prep::{self, CapturePrepInputs, CapturePrepOutcome};

pub(super) struct CaptureAndRegisterArgs<'a> {
    pub(super) state: &'a AppState,
    pub(super) parts: &'a axum::http::request::Parts,
    pub(super) body_bytes: &'a bytes::Bytes,
    pub(super) upstream_body: &'a bytes::Bytes,
    pub(super) request_id: RequestId,
    pub(super) signals: RequestSignals,
    pub(super) stored_body: Option<bytes::Bytes>,
    pub(super) json_body: Option<Value>,
    pub(super) observed_prompt_sha256: Option<String>,
    pub(super) observed_composition_id: Option<i64>,
    pub(super) observed_composition_applied: bool,
    pub(super) observed_unknown_blocks: Option<Value>,
    pub(super) filter_applied: bool,
    pub(super) firing_rule_ids: Vec<i64>,
    pub(super) manifest: crate::compress::manifest::ManifestBuilder,
    pub(super) injected_lesson_ids: Vec<i64>,
    pub(super) injected_reminders: Vec<SystemReminder>,
}

pub(super) struct CapturedPipelineTail {
    pub(super) request_id: RequestId,
    pub(super) session_id: String,
    pub(super) project_path: String,
    pub(super) streaming_model: Option<String>,
    pub(super) injected_reminders: Vec<SystemReminder>,
    pub(super) in_flight_guard: InFlightGuard,
}

pub(super) enum CaptureAndRegisterOutcome {
    Registered(CapturedPipelineTail),
    EarlyResponse(axum::response::Response),
}

pub(super) async fn capture_and_register(
    args: CaptureAndRegisterArgs<'_>,
) -> Result<CaptureAndRegisterOutcome, ProxyError> {
    let CaptureAndRegisterArgs {
        state,
        parts,
        body_bytes,
        upstream_body,
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
        mut manifest,
        injected_lesson_ids,
        injected_reminders,
    } = args;

    let lesson_injection_lane = if matches!(signals.turn_origin, TurnOrigin::Subagent) {
        "subagent"
    } else {
        "primary"
    };
    let lesson_injection_persona = signals.subagent_persona.clone();
    let mod_lesson_ids = crate::lesson_delivery::label_mod_lessons(body_bytes, &mut manifest);

    let CapturePrepOutcome {
        mut intercepted,
        streaming_model,
        body_tool_results,
        human_prompt_for_classification,
        is_new_session,
        session_id,
        project_path,
        turn_origin,
        turn_type,
    } = capture_prep::prepare_capture(CapturePrepInputs {
        state,
        parts,
        body_bytes,
        upstream_body,
        request_id,
        signals,
        stored_body,
        json_body: json_body.as_ref(),
        observed_prompt_sha256,
        observed_composition_id,
        observed_composition_applied,
        observed_unknown_blocks,
        filter_applied,
        firing_rule_ids,
        manifest,
    })
    .await;

    let request_id =
        capture::insert_stub_and_mint_id(&state.store, &mut intercepted, is_new_session).await;

    run_post_mint_attribution(PostMintAttributionArgs {
        state,
        request_id,
        session_id: &session_id,
        observed_composition_applied,
        lesson_injection_lane,
        lesson_injection_persona,
        injected_lesson_ids,
        mod_lesson_ids,
        injected_reminders: &injected_reminders,
        turn_origin: &turn_origin,
        turn_type: &turn_type,
        human_prompt_for_classification: &human_prompt_for_classification,
        classification_lane: ClassificationLane::of(&intercepted),
    })
    .await;

    spawn_mail_consumption(state, &session_id, body_bytes.clone());

    let in_flight_guard = match finish_capture_and_register(CaptureFinishArgs {
        state,
        request_id,
        session_id: &session_id,
        intercepted,
        body_tool_results,
        human_prompt_for_classification,
    })
    .await
    {
        CaptureFinishOutcome::Registered(guard) => guard,
        CaptureFinishOutcome::EarlyResponse(response) => {
            return Ok(CaptureAndRegisterOutcome::EarlyResponse(response));
        }
    };

    Ok(CaptureAndRegisterOutcome::Registered(
        CapturedPipelineTail {
            request_id,
            session_id,
            project_path,
            streaming_model,
            injected_reminders,
            in_flight_guard,
        },
    ))
}

fn spawn_mail_consumption(state: &AppState, session_id: &str, request_body: bytes::Bytes) {
    let (Some(persist), Ok(session)) = (&state.persist, session_id.parse::<uuid::Uuid>()) else {
        return;
    };
    let persist = persist.clone();
    state.tracker.spawn(async move {
        if let Err(e) = persist
            .consume_mail_carried_by(session, &request_body)
            .await
        {
            tracing::warn!(%session, error = %e, "marking carried mail consumed failed");
        }
        if let Err(e) = persist.revive_undelivered_mail(session).await {
            tracing::warn!(%session, error = %e, "reviving undelivered mail failed");
        }
    });
}

struct PostMintAttributionArgs<'a> {
    state: &'a AppState,
    request_id: RequestId,
    session_id: &'a str,
    observed_composition_applied: bool,
    lesson_injection_lane: &'static str,
    lesson_injection_persona: Option<String>,
    injected_lesson_ids: Vec<i64>,
    mod_lesson_ids: Vec<i64>,
    injected_reminders: &'a [SystemReminder],
    turn_origin: &'a TurnOrigin,
    turn_type: &'a TurnType,
    human_prompt_for_classification: &'a str,
    classification_lane: ClassificationLane<'a>,
}

struct ClassificationLane<'a> {
    last_user_message: &'a str,
    is_tool_loop: bool,
    has_agent_id: bool,
}

impl<'a> ClassificationLane<'a> {
    fn of(intercepted: &'a InterceptedRequest) -> Self {
        Self {
            last_user_message: &intercepted.last_user_message,
            is_tool_loop: intercepted.is_tool_loop,
            has_agent_id: intercepted.claude_code_agent_id.is_some(),
        }
    }
}

async fn run_post_mint_attribution(args: PostMintAttributionArgs<'_>) {
    let PostMintAttributionArgs {
        state,
        request_id,
        session_id,
        observed_composition_applied,
        lesson_injection_lane,
        lesson_injection_persona,
        injected_lesson_ids,
        mod_lesson_ids,
        injected_reminders,
        turn_origin,
        turn_type,
        human_prompt_for_classification,
        classification_lane,
    } = args;

    attribution::spawn_alter_decision_stamp(state, observed_composition_applied, session_id);
    attribution::spawn_mod_lesson_attribution(
        state,
        session_id,
        lesson_injection_lane,
        mod_lesson_ids,
        request_id,
    );
    attribution::spawn_lesson_injection_attribution(
        state,
        session_id,
        lesson_injection_lane,
        lesson_injection_persona,
        injected_lesson_ids,
        request_id,
    );
    attribution::stamp_steerer_sent(state, request_id, injected_reminders).await;

    let session_db_id = state.store.read().await.session_id(session_id);
    let author = classify_author_kind(
        &turn_origin.to_string(),
        human_prompt_for_classification,
        classification_lane.last_user_message,
        classification_lane.is_tool_loop,
        classification_lane.has_agent_id,
    );
    crate::store::operator_turns::record_prompt_kind_classification(
        state.persist.as_ref(),
        crate::store::operator_turns::PromptKindClassificationInput {
            turn_origin,
            turn_type,
            request_id,
            session_id,
            session_db_id,
            human_prompt: human_prompt_for_classification,
            last_user_message: classification_lane.last_user_message,
            author,
            has_agent_id: classification_lane.has_agent_id,
        },
    )
    .await;
}

struct CaptureFinishArgs<'a> {
    state: &'a AppState,
    request_id: RequestId,
    session_id: &'a str,
    intercepted: InterceptedRequest,
    body_tool_results: Vec<ToolResult>,
    human_prompt_for_classification: String,
}

enum CaptureFinishOutcome {
    Registered(InFlightGuard),
    EarlyResponse(axum::response::Response),
}

async fn finish_capture_and_register(args: CaptureFinishArgs<'_>) -> CaptureFinishOutcome {
    let CaptureFinishArgs {
        state,
        request_id,
        session_id,
        intercepted,
        body_tool_results,
        human_prompt_for_classification,
    } = args;

    let attach_outcome = capture::insert_and_attach_tool_results(
        &state.store,
        intercepted,
        session_id,
        body_tool_results,
    )
    .await;

    if !human_prompt_for_classification.is_empty() {
        let _ = state.event_tx.send(ProxyEvent::UserPromptReady {
            request_id,
            session_id: session_id.to_string(),
            user_text: human_prompt_for_classification,
        });
    }

    let in_flight_guard = match capture::register_in_flight(
        &state.in_flight,
        &state.store,
        &state.event_tx,
        request_id,
        session_id,
    ) {
        capture::InFlightRegistration::Registered(guard) => guard,
        capture::InFlightRegistration::CapacityExceeded { max } => {
            return CaptureFinishOutcome::EarlyResponse(
                crate::proxy::error_response::service_unavailable_capacity(max),
            );
        }
    };

    log_tool_result_db_fallback_candidates(session_id, attach_outcome);
    CaptureFinishOutcome::Registered(in_flight_guard)
}

fn log_tool_result_db_fallback_candidates(
    session_id: &str,
    attach_outcome: AttachToolResultsOutcome,
) {
    if !attach_outcome.db_fallback_candidates.is_empty() {
        for result in &attach_outcome.db_fallback_candidates {
            tracing::debug!(
                session_id = %session_id,
                tool_use_id = %result.tool_use_id,
                "tool_result missed in-memory session matcher; FK-shaped DB write in snapshot_request will resolve via tool_uses(tool_id)"
            );
        }
    }
    let _ = attach_outcome.touched;
}
