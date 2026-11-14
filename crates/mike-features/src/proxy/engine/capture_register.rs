use crate::store::persistence::launch_repo::LaunchPersist as _;
use serde_json::Value;

use crate::capture;
use crate::error::ProxyError;
use crate::extract::request::{TurnOrigin, TurnType};
use crate::extract::signals::RequestSignals;
use crate::in_flight::InFlightGuard;
use crate::state::AppState;
use crate::store::AttachToolResultsOutcome;
use crate::store::models::{InterceptedRequest, ProxyEvent, RequestId, SystemReminder, ToolResult};
use crate::store::persistence::capture_classification::classify_author_kind;

use super::attribution;
use super::capture_prep::{self, CapturePrepInputs, CapturePrepOutcome};
use CaptureAndRegisterOutcome as Outcome;

pub struct CaptureAndRegisterArgs<'a> {
    pub state: &'a AppState,
    pub parts: &'a axum::http::request::Parts,
    pub body_bytes: &'a bytes::Bytes,
    pub upstream_body: &'a bytes::Bytes,
    pub request_id: RequestId,
    pub signals: RequestSignals,
    pub stored_body: Option<bytes::Bytes>,
    pub json_body: Option<Value>,
    pub observed_prompt_sha256: Option<String>,
    pub observed_composition_id: Option<i64>,
    pub observed_composition_applied: bool,
    pub observed_unknown_blocks: Option<Value>,
    pub filter_applied: bool,
    pub firing_rule_ids: Vec<i64>,
    pub manifest: crate::wire::manifest::ManifestBuilder,
    pub injected_lesson_ids: Vec<i64>,
    pub injected_reminders: Vec<SystemReminder>,
}

pub struct CapturedPipelineTail {
    pub request_id: RequestId,
    pub session_id: String,
    pub project_path: String,
    pub streaming_model: Option<String>,
    pub injected_reminders: Vec<SystemReminder>,
    pub in_flight_guard: InFlightGuard,
}

pub enum CaptureAndRegisterOutcome {
    Registered(CapturedPipelineTail),
    EarlyResponse(axum::response::Response),
}

fn lesson_labels(
    state: &AppState,
    signals: &RequestSignals,
    body: &bytes::Bytes,
    manifest: &mut crate::wire::manifest::ManifestBuilder,
) -> (&'static str, Option<String>, Vec<i64>) {
    let lane = if matches!(signals.turn_origin, TurnOrigin::Subagent) {
        "subagent"
    } else {
        "primary"
    };
    let mod_lessons = state.lesson_hooks().label_mod_lessons(body, manifest);
    (lane, signals.subagent_persona.clone(), mod_lessons)
}

fn spawn_supply_records(
    state: &AppState,
    request_id: RequestId,
    session_id: &str,
    (original, sent): (&bytes::Bytes, &bytes::Bytes),
    proxy_lessons: &[i64],
) {
    state.lesson_hooks().spawn_supply_records(
        state,
        crate::state::lesson_hooks::SupplyCapture {
            request_id,
            session_id,
            original,
            sent,
            proxy_lessons,
        },
    );
    spawn_mail_consumption(state, session_id, original.clone());
}

pub async fn capture_and_register(
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

    let (lesson_injection_lane, lesson_injection_persona, mod_lesson_ids) =
        lesson_labels(state, &signals, body_bytes, &mut manifest);

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

    let request_id = mint_capture_id(state, &mut intercepted, is_new_session, request_id).await;

    spawn_supply_records(
        state,
        request_id,
        &session_id,
        (body_bytes, upstream_body),
        &injected_lesson_ids,
    );
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
            return Ok(Outcome::EarlyResponse(response));
        }
    };

    Ok(Outcome::Registered(CapturedPipelineTail {
        request_id,
        session_id,
        project_path,
        streaming_model,
        injected_reminders,
        in_flight_guard,
    }))
}

fn spawn_mail_consumption(state: &AppState, session_id: &str, request_body: bytes::Bytes) {
    let (Some(persist), Ok(session)) = (&state.persist, session_id.parse::<uuid::Uuid>()) else {
        return;
    };
    let persist = persist.clone();
    crate::observability::spans::spawn_tracked(&state.tracker, async move {
        if let Err(e) = persist
            .launch()
            .consume_mail_carried_by(session, &request_body)
            .await
        {
            tracing::warn!(session_id = %session, error = %e, "marking carried mail consumed failed");
        }
        if let Err(e) = persist.launch().revive_undelivered_mail(session).await {
            tracing::warn!(session_id = %session, error = %e, "reviving undelivered mail failed");
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
    state.lesson_hooks().attribute(
        state,
        crate::state::lesson_hooks::LessonAttribution {
            session_id,
            lane: lesson_injection_lane,
            persona: lesson_injection_persona,
            injected: injected_lesson_ids,
            mod_lessons: mod_lesson_ids,
            request_id,
        },
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
    let recorded = crate::store::operator_turns::record_prompt_kind_classification(
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
    spawn_text_origin(
        state,
        recorded,
        human_prompt_for_classification,
        &classification_lane,
        author,
    );
}

fn spawn_text_origin(
    state: &AppState,
    recorded: Option<crate::store::operator_turns::RecordedPrompt>,
    text: &str,
    lane: &ClassificationLane<'_>,
    author: &str,
) {
    use crate::wire::genuine::PromptKind;
    let Some(recorded) =
        recorded.filter(|r| r.class.kind == PromptKind::Genuine && r.class.rule.is_none())
    else {
        return;
    };
    let Some(env) = state.decision_env() else {
        return;
    };
    let input = crate::llm::text_origin::Unmatched {
        request_id: recorded.request_id,
        session_db_id: recorded.session_db_id,
        text: text.to_owned(),
        last_user_message: lane.last_user_message.to_owned(),
        author: author.to_owned(),
        has_agent_id: lane.has_agent_id,
    };
    crate::observability::spans::spawn_tracked(&state.tracker, async move {
        crate::llm::text_origin::classify(&env, &input).await;
    });
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

async fn mint_capture_id(
    state: &AppState,
    intercepted: &mut InterceptedRequest,
    is_new_session: bool,
    pipeline_request: RequestId,
) -> RequestId {
    let capture_request =
        capture::insert_stub_and_mint_id(&state.store, intercepted, is_new_session).await;
    bind_code_notes(
        state,
        pipeline_request,
        capture_request,
        &intercepted.session_id,
    );
    capture_request
}

fn bind_code_notes(
    state: &AppState,
    pipeline_request: RequestId,
    capture_request: RequestId,
    session_id: &str,
) {
    if mike_base::store::models::request_id::is_fallback(capture_request.as_u64()) {
        return;
    }
    let capture = i64::try_from(capture_request.as_u64()).unwrap_or(i64::MAX);
    state
        .code_notes
        .bind_capture_id(pipeline_request.as_u64(), capture);
    if let Ok(session) = session_id.parse::<uuid::Uuid>() {
        state.code_notes.bind_session_request(session, capture);
    }
}
