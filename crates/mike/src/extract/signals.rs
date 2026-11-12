use axum::http::request::Parts;
use bytes::Bytes;
use serde_json::Value;

use super::request::{self, TurnOrigin, TurnType};
use crate::config::AppState;
use crate::extract::headers::{is_sensitive_header, mask_header_value};
use crate::proxy::handler::{MAX_HEADER_BYTES, MAX_HEADERS};
use crate::types::RequestParams;

#[derive(Debug, Clone)]
pub(crate) struct RequestSignals {
    pub(crate) session_id: String,
    pub(crate) model: Option<String>,
    pub(crate) is_streaming: bool,
    pub(crate) message_count: usize,
    pub(crate) is_tool_loop: bool,
    pub(crate) last_user_message: String,
    pub(crate) tool_names: Vec<String>,
    pub(crate) params: RequestParams,
    pub(crate) human_prompt: String,
    pub(crate) turn_type: TurnType,
    pub(crate) turn_origin: TurnOrigin,
    pub(crate) subagent_persona: Option<String>,
    pub(crate) request_headers: Vec<(String, String)>,
    pub(crate) dropped_header_count: usize,
    pub(crate) dropped_header_bytes: usize,
    pub(crate) claude_code_agent_id: Option<String>,
    pub(crate) project_path: String,
    pub(crate) mutation_exempt: bool,
}

struct BasicFields {
    model: Option<String>,
    is_streaming: bool,
    message_count: usize,
    is_tool_loop: bool,
    last_user_message: String,
    system_prompt: Option<String>,
    tool_names: Vec<String>,
    params: RequestParams,
    human_prompt: String,
    turn_type: TurnType,
}

impl BasicFields {
    fn extract(json_body: Option<&Value>) -> Self {
        Self {
            model: json_body.and_then(request::extract_model),
            is_streaming: json_body.map(request::extract_stream).unwrap_or(false),
            message_count: json_body.map(request::extract_message_count).unwrap_or(0),
            is_tool_loop: json_body.map(request::detect_tool_loop).unwrap_or(false),
            last_user_message: json_body
                .map(request::extract_last_user_message)
                .unwrap_or_default(),
            system_prompt: json_body.and_then(request::extract_system_prompt),
            tool_names: json_body
                .map(request::extract_tool_names)
                .unwrap_or_default(),
            params: json_body
                .map(request::extract_params)
                .unwrap_or(RequestParams {
                    max_tokens: None,
                    temperature: None,
                    thinking_enabled: false,
                    thinking_budget: None,
                    thinking_mode: None,
                    effort: None,
                }),
            human_prompt: json_body
                .map(request::extract_human_prompt)
                .unwrap_or_default(),
            turn_type: json_body
                .map(request::extract_turn_type)
                .unwrap_or(TurnType::Continuation),
        }
    }
}

struct TurnClassificationInput<'a> {
    json_body: Option<&'a Value>,
    model: Option<&'a str>,
    system_prompt: Option<&'a str>,
    turn_type: &'a TurnType,
    has_agent_id: bool,
}

fn agent_lane_origin(origin: TurnOrigin, input: &TurnClassificationInput<'_>) -> TurnOrigin {
    if !input.has_agent_id || origin != TurnOrigin::Primary {
        return origin;
    }
    let workflow = input
        .json_body
        .is_some_and(|body| request::is_workflow_agent(input.system_prompt, body));
    if workflow {
        TurnOrigin::WorkflowSubagent
    } else {
        TurnOrigin::Subagent
    }
}

impl RequestSignals {
    pub(crate) async fn from_parts_and_body(
        parts: &Parts,
        body_bytes: &Bytes,
        state: &AppState,
    ) -> Result<Self, request::SessionIdMiss> {
        let json_body: Option<Value> = serde_json::from_slice(body_bytes).ok();
        let session_id = request::derive_session_id(&parts.headers)?;
        let basic = BasicFields::extract(json_body.as_ref());

        let claude_code_agent_id = extract_claude_code_agent_id(parts);
        let (turn_origin, subagent_persona) = classify_turn_origin_and_persona(
            parts,
            state,
            &session_id,
            TurnClassificationInput {
                json_body: json_body.as_ref(),
                model: basic.model.as_deref(),
                system_prompt: basic.system_prompt.as_deref(),
                turn_type: &basic.turn_type,
                has_agent_id: claude_code_agent_id.is_some(),
            },
        )
        .await;

        let (request_headers, dropped_header_count, dropped_header_bytes) =
            capture_request_headers(parts);
        let project_path =
            derive_project_signals(json_body.as_ref(), basic.system_prompt.as_deref());
        let mutation_exempt = crate::extract::headers::is_mutation_exempt_client(&parts.headers);

        Ok(Self {
            session_id,
            model: basic.model,
            is_streaming: basic.is_streaming,
            message_count: basic.message_count,
            is_tool_loop: basic.is_tool_loop,
            last_user_message: basic.last_user_message,
            tool_names: basic.tool_names,
            params: basic.params,
            human_prompt: basic.human_prompt,
            turn_type: basic.turn_type,
            turn_origin,
            subagent_persona,
            request_headers,
            dropped_header_count,
            dropped_header_bytes,
            claude_code_agent_id,
            project_path,
            mutation_exempt,
        })
    }
}

async fn read_established_persona(state: &AppState, session_id: &str) -> Option<Option<String>> {
    let store = state.store.read().await;
    store.established_session_persona(session_id)
}

async fn classify_turn_origin_and_persona(
    parts: &Parts,
    state: &AppState,
    session_id: &str,
    input: TurnClassificationInput<'_>,
) -> (TurnOrigin, Option<String>) {
    let established_persona = read_established_persona(state, session_id).await;
    let session_identity_established = established_persona.is_some();
    let session_identity_persona: Option<&str> = established_persona
        .as_ref()
        .and_then(|persona| persona.as_deref());

    let mut turn_origin = input
        .json_body
        .map(|b| {
            request::extract_turn_origin(
                b,
                input.system_prompt,
                input.model,
                input.turn_type,
                session_identity_established,
                session_identity_persona,
            )
        })
        .unwrap_or(TurnOrigin::Primary);

    if parts.uri.path().ends_with("/count_tokens") {
        turn_origin = TurnOrigin::CountTokensSidecar;
    }

    turn_origin = agent_lane_origin(turn_origin, &input);

    if turn_origin == TurnOrigin::Primary {
        turn_origin = maybe_promote_to_primary_first(state, session_id).await;
    }

    if established_persona.is_none()
        && matches!(turn_origin, TurnOrigin::Primary | TurnOrigin::PrimaryFirst)
    {
        let this_persona = input
            .system_prompt
            .and_then(request::extract_subagent_persona);
        let mut store = state.store.write().await;
        store.establish_session_persona(session_id, this_persona);
    }

    let subagent_persona = match turn_origin {
        TurnOrigin::Subagent => input
            .system_prompt
            .and_then(request::extract_subagent_persona),
        _ => None,
    };

    (turn_origin, subagent_persona)
}

async fn maybe_promote_to_primary_first(state: &AppState, session_id: &str) -> TurnOrigin {
    let maybe_first = {
        let store = state.store.read().await;
        store.is_first_composable_request_in_session(session_id)
    };
    let is_first = if maybe_first {
        let mut store = state.store.write().await;
        store.claim_first_composable_slot(session_id)
    } else {
        false
    };
    if !is_first {
        return TurnOrigin::Primary;
    }

    if state.persist.is_some()
        && let Ok(session_uuid) = uuid::Uuid::parse_str(session_id)
    {
        let reload_state = state.clone();
        state.tracker.spawn(async move {
            crate::startup::reload_mailbox_for_session(&reload_state, session_uuid).await;
        });
    }
    TurnOrigin::PrimaryFirst
}

enum HeaderCapture {
    Kept(String, String),
    DroppedForBytes,
}

fn decode_header_pair(
    name: &axum::http::HeaderName,
    value: &axum::http::HeaderValue,
) -> HeaderCapture {
    let name_str = name.as_str().to_string();
    let raw = match value.to_str() {
        Ok(s) => s,
        Err(_) => {
            tracing::debug!(
                name = %name,
                "header value contained non-UTF-8 bytes; storing as empty string"
            );
            ""
        }
    };
    let value_str = if is_sensitive_header(name.as_str()) {
        mask_header_value(raw)
    } else {
        raw.to_string()
    };
    if name_str.len().saturating_add(value_str.len()) > MAX_HEADER_BYTES {
        return HeaderCapture::DroppedForBytes;
    }
    HeaderCapture::Kept(name_str, value_str)
}

fn log_header_capture_overflow(
    kept: usize,
    dropped_header_count: usize,
    dropped_header_bytes: usize,
) {
    if dropped_header_count == 0 && dropped_header_bytes == 0 {
        return;
    }
    tracing::debug!(
        kept,
        dropped_for_count = dropped_header_count,
        dropped_for_bytes = dropped_header_bytes,
        max_headers = MAX_HEADERS,
        max_header_bytes = MAX_HEADER_BYTES,
        "request_headers capture cap hit; overflow pairs dropped from inspection-side projection (upstream sees the original headers unmodified)"
    );
}

fn capture_request_headers(parts: &Parts) -> (Vec<(String, String)>, usize, usize) {
    let mut request_headers: Vec<(String, String)> = Vec::with_capacity(MAX_HEADERS);
    let mut dropped_header_count: usize = 0;
    let mut dropped_header_bytes: usize = 0;
    for (name, value) in parts.headers.iter() {
        match decode_header_pair(name, value) {
            HeaderCapture::DroppedForBytes => dropped_header_bytes += 1,
            HeaderCapture::Kept(_, _) if request_headers.len() >= MAX_HEADERS => {
                dropped_header_count += 1;
            }
            HeaderCapture::Kept(name_str, value_str) => {
                request_headers.push((name_str, value_str));
            }
        }
    }
    log_header_capture_overflow(
        request_headers.len(),
        dropped_header_count,
        dropped_header_bytes,
    );
    (request_headers, dropped_header_count, dropped_header_bytes)
}

fn extract_claude_code_agent_id(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get("x-claude-code-agent-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn derive_project_signals(json_body: Option<&Value>, system_prompt: Option<&str>) -> String {
    let mut message_sources: Vec<&str> = Vec::new();
    if let Some(b) = json_body {
        message_sources.extend(request::collect_project_scan_sources(b));
    }
    request::extract_project(request::ProjectSignals {
        system_prompt,
        message_sources,
    })
    .unwrap_or_default()
}

#[cfg(test)]
#[path = "signals_agent_id_tests.rs"]
mod agent_id_tests;
#[cfg(test)]
#[path = "signals_tests.rs"]
mod tests;
