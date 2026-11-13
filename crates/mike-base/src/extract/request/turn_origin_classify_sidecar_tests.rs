use super::*;
use serde_json::json;

use super::super::{extract_model, extract_system_prompt, extract_turn_type};

#[test]
fn turn_origin_title_sidecar_haiku_one_shot_with_markers() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "Summarize this diff"}
        ]
    });
    let sp = "Generate a concise, sentence-case title for the conversation. Respond with JSON like {\"title\": \"...\"}.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-haiku-4-5"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::TitleSidecar);
    assert_eq!(origin.to_string(), "title_sidecar");
}

#[test]
fn turn_origin_title_sidecar_naming_a_coding_session_wording() {
    let body = json!({
        "model": "claude-haiku-4-5-20251001",
        "max_tokens": 1024,
        "messages": [
            {"role": "user", "content": [
                {"type": "text",
                 "text": "<session>\nget from the triage map the next set of tasks to work on.\n</session>\n\nWrite the title in the predominant language of the session."}
            ]}
        ]
    });
    let sp = concat!(
        "x-anthropic-billing-header: cc_version=2.1.237.486; cc_entrypoint=cli;\n",
        "You are Claude Code, Anthropic's official CLI for Claude.\n",
        "You are naming a coding session so the user can pick it out of a long ",
        "list of sessions. The title is a name for what the session is about, ",
        "not a sentence describing the task.\n",
        "The session content is provided inside <session> tags.\n",
        "Return JSON with a single \"title\" field. Capitalize the first letter ",
        "of the title.",
    );
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-haiku-4-5-20251001"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::TitleSidecar);
    assert!(!origin.stamps_capture_identity());
}

#[test]
fn turn_origin_title_sidecar_from_captured_wire_body() -> anyhow::Result<()> {
    const CAPTURED: &str =
        include_str!("../../../tests/fixtures/turn_origin/title_sidecar_cc_2_1_237.json");
    let body: Value = serde_json::from_str(CAPTURED)?;
    let sp = extract_system_prompt(&body);
    let turn_type = extract_turn_type(&body);
    let origin = extract_turn_origin(
        &body,
        sp.as_deref(),
        extract_model(&body).as_deref(),
        &turn_type,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::TitleSidecar);
    assert!(!origin.stamps_capture_identity());
    Ok(())
}

#[test]
fn turn_origin_primary_when_title_word_present_without_lead_marker() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "rename the page"}
        ]
    });
    let sp = "You are Claude Code. The page's \"title\" element is wrong.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-opus-5"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Primary);
}

#[test]
fn turn_origin_primary_when_turn_type_is_tool_loop() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "do the thing"},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "bash", "input": {"command": "ls"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "file.txt"}
            ]}
        ]
    });
    let sp = "Generate a concise, sentence-case title for the conversation. Respond with JSON {\"title\": \"...\"}.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-haiku-4-5"),
        &TurnType::ToolLoop,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Primary);
}

#[test]
fn turn_origin_primary_when_turn_type_is_continuation() {
    let body = json!({
        "messages": [
            {"role": "assistant", "content": "nothing to see here"}
        ]
    });
    let origin = extract_turn_origin(&body, None, None, &TurnType::Continuation, true, None);
    assert_eq!(origin, TurnOrigin::Primary);
}

#[test]
fn turn_origin_primary_when_message_count_gt_one() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "first turn"},
            {"role": "assistant", "content": "response"},
            {"role": "user", "content": "follow-up"}
        ]
    });
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are an interactive agent that helps users with software engineering tasks.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-opus-4-7"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Primary);
}

#[test]
fn turn_origin_primary_when_system_prompt_absent() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "hello"}
        ]
    });
    let origin = extract_turn_origin(
        &body,
        None,
        Some("claude-opus-4-7"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Primary);
}

#[test]
fn turn_origin_primary_when_haiku_without_title_markers() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "summarize this"}
        ]
    });
    let sp = "You are a helpful assistant. Be concise.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-haiku-4-5"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Primary);
}

#[test]
fn turn_origin_title_sidecar_on_non_haiku_model() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "summarize"}
        ]
    });
    let sp = "Generate a concise, sentence-case title. Respond with {\"title\": \"...\"}.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-sonnet-4-20250514"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::TitleSidecar);
}

#[test]
fn turn_origin_advisor_sidecar_security_monitor_with_transcript() {
    let body = json!({
        "max_tokens": 64,
        "messages": [
            {"role": "user", "content": "<transcript>\nUser: fix the login bug\n</transcript>"},
            {"role": "user", "content": "<block>no</block>"}
        ]
    });
    let sp = "x-anthropic-billing-header: cc_version=2.1.112; cch=184ed;\nYou are a security monitor for autonomous AI coding agents.\n\n## Context\n\n…";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-opus-4-7"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::AdvisorSidecar);
    assert_eq!(origin.to_string(), "advisor_sidecar");
}

#[test]
fn turn_origin_primary_when_security_monitor_but_large_max_tokens() {
    let body = json!({
        "max_tokens": 4000,
        "messages": [
            {"role": "user", "content": "first"},
            {"role": "user", "content": "second"}
        ]
    });
    let sp = "You are a security monitor for autonomous AI coding agents.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-opus-4-7"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Primary);
}

#[test]
fn turn_origin_primary_when_security_monitor_but_only_one_message() {
    let body = json!({
        "max_tokens": 64,
        "messages": [
            {"role": "user", "content": "<transcript>User: hi</transcript>"}
        ]
    });
    let sp = "You are a security monitor for autonomous AI coding agents.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-opus-4-7"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::Primary);
}

#[test]
fn turn_origin_advisor_sidecar_wins_over_subagent_if_both_markers_present() {
    let body = json!({
        "max_tokens": 64,
        "messages": [
            {"role": "user", "content": "first"},
            {"role": "user", "content": "<transcript>User: hi</transcript>"}
        ]
    });
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are a Frontend Development Expert.\nYou are a security monitor for autonomous AI coding agents.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-opus-4-7"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::AdvisorSidecar);
}

#[test]
fn turn_origin_title_sidecar_on_opus_4_8() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "summarize this conversation"}
        ]
    });
    let sp = "Generate a concise, sentence-case title. Respond only with JSON of the form {\"title\": \"...\"}.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-opus-4-8"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::TitleSidecar);
    assert_eq!(origin.to_string(), "title_sidecar");
}

#[test]
fn turn_origin_title_sidecar_with_trailing_system_message() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "summarize this conversation"},
            {"role": "system", "content": "mid-session policy update"}
        ]
    });
    let sp = "Generate a concise, sentence-case title. Respond only with JSON of the form {\"title\": \"...\"}.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-opus-4-8"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::TitleSidecar);
}

#[test]
fn turn_origin_advisor_sidecar_with_trailing_system_message() {
    let body = json!({
        "max_tokens": 64,
        "messages": [
            {"role": "user", "content": "transcript here"},
            {"role": "assistant", "content": "advisor verdict"},
            {"role": "system", "content": "mid-session policy update"}
        ]
    });
    let sp = "You are a security monitor for autonomous AI coding agents.";
    let origin = extract_turn_origin(
        &body,
        Some(sp),
        Some("claude-opus-4-8"),
        &TurnType::UserPrompt,
        true,
        None,
    );
    assert_eq!(origin, TurnOrigin::AdvisorSidecar);
}
