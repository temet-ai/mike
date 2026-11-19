use super::*;
use serde_json::{Value, json};

const SUBAGENT_SYSTEM: &str =
    "You are Claude Code, Anthropic's official CLI for Claude.\ncc_is_subagent=true";

fn label_text() -> String {
    "<task>\nRun a one-off check.\n</task>\n<recent_steps>\ndid: Bash - ls\n</recent_steps>\n\nDescribe the agent's most recent action in 3-5 words using present tense (-ing).".to_owned()
}

fn origin_of(body: &Value) -> TurnOrigin {
    extract_turn_origin(
        body,
        Some(SUBAGENT_SYSTEM),
        Some("claude-haiku-4-5"),
        &TurnType::UserPrompt,
        true,
        None,
    )
}

#[test]
fn agent_label_call_with_subagent_system_is_label_sidecar() {
    let body = json!({
        "messages": [{"role": "user", "content": [{"type": "text", "text": label_text()}]}]
    });
    let origin = origin_of(&body);
    assert_eq!(origin, TurnOrigin::AgentLabelSidecar);
    assert_eq!(origin.to_string(), "agent_label_sidecar");
}

#[test]
fn agent_label_call_carrying_a_thread_stays_subagent() {
    let body = json!({
        "thread": {"type": "continue", "previous_message_id": "msg_1"},
        "messages": [{"role": "user", "content": [{"type": "text", "text": label_text()}]}]
    });
    assert_eq!(origin_of(&body), TurnOrigin::Subagent);
}

#[test]
fn task_prefix_without_label_instruction_stays_subagent() {
    let body = json!({
        "messages": [{"role": "user", "content": "<task>\nFix the parser.\n</task>"}]
    });
    assert_eq!(origin_of(&body), TurnOrigin::Subagent);
}

#[test]
fn label_text_after_earlier_messages_stays_subagent() {
    let body = json!({
        "messages": [
            {"role": "user", "content": "start"},
            {"role": "assistant", "content": "ok"},
            {"role": "user", "content": label_text()}
        ]
    });
    assert_eq!(origin_of(&body), TurnOrigin::Subagent);
}
