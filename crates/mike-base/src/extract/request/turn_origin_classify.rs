use serde_json::Value;

use super::message_shape::fingerprint_message_count;
use super::turn_origin::TurnOrigin;
use super::turn_type::TurnType;
use crate::wire::genuine::markers::{WORKFLOW_HARNESS_USER_PREFIX, WORKFLOW_SUBAGENT_MARKERS};

fn first_user_message_text(body: &Value) -> Option<&str> {
    let msg = body
        .get("messages")
        .and_then(|v| v.as_array())
        .and_then(|msgs| msgs.first())?;
    if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
        return None;
    }
    msg.get("content").and_then(|c| c.as_str())
}

fn first_user_single_text(body: &Value) -> Option<&str> {
    let msg = body
        .get("messages")
        .and_then(|v| v.as_array())
        .and_then(|msgs| msgs.first())?;
    if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
        return None;
    }
    match msg.get("content")? {
        Value::String(s) => Some(s.as_str()),
        Value::Array(blocks) if blocks.len() == 1 => {
            let block = &blocks[0];
            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                block.get("text").and_then(|t| t.as_str())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn last_user_message_plain_string(body: &Value) -> Option<&str> {
    let msg = body
        .get("messages")
        .and_then(|v| v.as_array())
        .and_then(|msgs| {
            msgs.iter()
                .rev()
                .find(|msg| msg.get("role").and_then(|r| r.as_str()) == Some("user"))
        })?;
    msg.get("content").and_then(|c| c.as_str())
}

fn has_workflow_system_marker(system_prompt: &str) -> bool {
    WORKFLOW_SUBAGENT_MARKERS
        .iter()
        .any(|marker| system_prompt.contains(marker))
}

fn starts_with_workflow_harness_prefix(text: &str) -> bool {
    text.trim_start().starts_with(WORKFLOW_HARNESS_USER_PREFIX)
}

fn first_user_message_starts_with_harness_prefix(body: &Value) -> bool {
    let Some(msg) = body
        .get("messages")
        .and_then(|v| v.as_array())
        .and_then(|msgs| msgs.first())
    else {
        return false;
    };
    if msg.get("role").and_then(|r| r.as_str()) != Some("user") {
        return false;
    }
    match msg.get("content") {
        Some(Value::String(text)) => starts_with_workflow_harness_prefix(text),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(|t| t.as_str()))
            .any(starts_with_workflow_harness_prefix),
        _ => false,
    }
}

pub fn is_workflow_agent(system_prompt: Option<&str>, body: &Value) -> bool {
    system_prompt.is_some_and(has_workflow_system_marker)
        || first_user_message_starts_with_harness_prefix(body)
}

pub fn extract_turn_origin(
    body: &Value,
    system_prompt: Option<&str>,
    _model: Option<&str>,
    turn_type: &TurnType,
    session_identity_established: bool,
    session_identity_persona: Option<&str>,
) -> TurnOrigin {
    let sp = system_prompt.unwrap_or("");
    let message_count = fingerprint_message_count(body);
    let max_tokens = body
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(u64::MAX);

    if message_count == 2
        && max_tokens <= 128
        && sp.contains("You are a security monitor for autonomous AI coding agents")
    {
        return TurnOrigin::AdvisorSidecar;
    }

    if let Some(origin) =
        subagent_lane_origin(sp, session_identity_established, session_identity_persona)
    {
        return origin;
    }

    if *turn_type != TurnType::UserPrompt {
        return TurnOrigin::Primary;
    }

    if let Some(text) = last_user_message_plain_string(body)
        && text.trim_start().starts_with("[SUGGESTION MODE:")
    {
        return TurnOrigin::NextInputSuggestionSidecar;
    }

    if message_count > 1 {
        return TurnOrigin::Primary;
    }

    if max_tokens == 1 && first_user_message_text(body) == Some("quota") {
        return TurnOrigin::QuotaProbe;
    }

    if let Some(text) = first_user_single_text(body)
        && text.trim_start().starts_with("Web page content:\n---")
    {
        return TurnOrigin::WebFetchSidecar;
    }

    if sp.contains("A user kicked off a Claude Code agent to do a coding task and walked away") {
        return TurnOrigin::MonitorSidecar;
    }

    if sp.contains("\"title\"")
        && (sp.contains("Generate a concise, sentence-case title")
            || sp.contains("You are naming a coding session")
            || sp.contains("Return JSON with a single \"title\" field"))
    {
        return TurnOrigin::TitleSidecar;
    }

    TurnOrigin::Primary
}

fn subagent_lane_origin(
    sp: &str,
    session_identity_established: bool,
    session_identity_persona: Option<&str>,
) -> Option<TurnOrigin> {
    if has_workflow_system_marker(sp) {
        return Some(TurnOrigin::WorkflowSubagent);
    }

    let has_cc_header =
        sp.starts_with("You are Claude Code") || sp.contains("\nYou are Claude Code");
    let has_persona_override = sp.contains("\nYou are a ") || sp.contains("\nYou are an ");
    let is_main_interactive_agent = sp.contains("interactive agent that helps users");

    if has_cc_header && has_persona_override && !is_main_interactive_agent {
        if !session_identity_established {
            return None;
        }
        let this_persona = extract_subagent_persona(sp);
        if session_identity_persona == this_persona.as_deref() {
            return None;
        }
        return Some(TurnOrigin::Subagent);
    }

    if !has_persona_override && sp.contains("cc_is_subagent=true") {
        return Some(TurnOrigin::Subagent);
    }

    None
}

pub fn extract_subagent_persona(system_prompt: &str) -> Option<String> {
    const MAIN_AGENT_MARKER: &str = "interactive agent that helps users";
    const AGENT_SDK_PREAMBLE_MARKER: &str = "built on Anthropic's Claude Agent SDK";

    for line in system_prompt.split('\n') {
        let Some(rest) = line
            .strip_prefix("You are a ")
            .or_else(|| line.strip_prefix("You are an "))
            .or_else(|| line.strip_prefix("You are the "))
        else {
            continue;
        };
        if line.contains(MAIN_AGENT_MARKER) || line.contains(AGENT_SDK_PREAMBLE_MARKER) {
            continue;
        }
        let dash_idx = rest.find(" — ").or_else(|| rest.find(" -- "));
        let punct_idx = rest.find(['.', ',']);
        let persona_end = match (dash_idx, punct_idx) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => rest.len(),
        };
        let persona = rest[..persona_end].trim();
        if persona.is_empty() {
            return None;
        }
        return Some(persona.to_string());
    }
    None
}

#[cfg(test)]
#[path = "turn_origin_classify_probe_tests.rs"]
pub mod probe_tests;
#[cfg(test)]
#[path = "turn_origin_classify_sidecar_tests.rs"]
pub mod sidecar_tests;
#[cfg(test)]
#[path = "turn_origin_classify_subagent_tests.rs"]
pub mod subagent_tests;
