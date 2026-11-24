use serde_json::{Value, json};

use super::tests_inject_reminder::{REMINDER_BODY, REMINDER_TOKEN, inject_reminder_rule};
use super::tests_support::{cfg, rewrote};
use super::*;
use crate::reminders::test_support::earlier_from_messages;

fn thread_body(messages: Vec<Value>) -> anyhow::Result<Vec<u8>> {
    Ok(serde_json::to_vec(&json!({
        "system": [{"type": "text", "text": "You are Claude Code."}],
        "thread": {"type": "continue", "previous_message_id": "msg_1"},
        "messages": messages,
    }))?)
}

fn opening() -> Vec<Value> {
    vec![
        json!({"role": "user", "content": [{"type": "text", "text": "open the page"}]}),
        json!({"role": "assistant", "content": [
            {"type": "text", "text": format!("Loading {REMINDER_TOKEN}")}
        ]}),
    ]
}

fn tool_result() -> Value {
    json!({"role": "user", "content": [
        {"type": "tool_result", "tool_use_id": "t1", "content": "tabs"}
    ]})
}

#[test]
fn inject_reminder_continue_body_with_earlier_trigger_injects_like_full_body() -> anyhow::Result<()>
{
    let rules = cfg(vec![inject_reminder_rule(42)]);
    let mut full_messages = opening();
    full_messages.push(tool_result());
    let full = serde_json::to_vec(&json!({
        "system": [{"type": "text", "text": "You are Claude Code."}],
        "messages": full_messages,
    }))?;
    let (whole, _, whole_injected, whole_firing) = rewrote(apply_lazy_sections(
        &full,
        &rules,
        "",
        &crate::conversation::WHOLE_BODY,
    ))?;

    let earlier = earlier_from_messages(&opening())?;
    let thread = thread_body(vec![tool_result()])?;
    let out = apply_lazy_sections(&thread, &rules, "", &earlier);
    let (chained, _, injected, firing) = rewrote(out)?;

    assert!(whole_injected && injected);
    assert_eq!(firing, whole_firing);
    assert_eq!(
        chained["messages"][0]["content"],
        whole["messages"][2]["content"]
    );
    assert_eq!(
        chained["messages"][0]["content"][1]["text"]
            .as_str()
            .map(|t| t.contains(REMINDER_BODY)),
        Some(true)
    );
    assert!(matches!(
        apply_lazy_sections(&thread, &rules, "", &crate::conversation::WHOLE_BODY),
        LazySectionsOutcome::Unchanged
    ));
    Ok(())
}

#[test]
fn inject_reminder_continue_body_with_trigger_in_older_earlier_assistant_is_noop()
-> anyhow::Result<()> {
    let rules = cfg(vec![inject_reminder_rule(42)]);
    let mut history = opening();
    history.push(tool_result());
    history.push(json!({"role": "assistant", "content": [{"type": "text", "text": "done"}]}));
    let earlier = earlier_from_messages(&history)?;
    let thread = thread_body(vec![
        json!({"role": "user", "content": [{"type": "text", "text": "next"}]}),
    ])?;
    assert!(matches!(
        apply_lazy_sections(&thread, &rules, "", &earlier),
        LazySectionsOutcome::Unchanged
    ));
    Ok(())
}

#[test]
fn has_thinking_blocks_continue_body_with_earlier_thinking_block_is_true() -> anyhow::Result<()> {
    let earlier = earlier_from_messages(&[
        json!({"role": "user", "content": "q"}),
        json!({"role": "assistant", "content": [
            {"type": "thinking", "thinking": "hm", "signature": "sig"},
            {"type": "text", "text": "a"}
        ]}),
    ])?;
    let body: Value = serde_json::from_slice(&thread_body(vec![tool_result()])?)?;
    assert!(has_thinking_blocks(&body, &earlier));
    assert!(!has_thinking_blocks(
        &body,
        &crate::conversation::WHOLE_BODY
    ));
    Ok(())
}

#[test]
fn lazy_sections_continue_body_with_earlier_thinking_block_is_left_unchanged() -> anyhow::Result<()>
{
    let rules = cfg(vec![inject_reminder_rule(42)]);
    let mut history = opening();
    history[1] = json!({"role": "assistant", "content": [
        {"type": "thinking", "thinking": "hm", "signature": "sig"},
        {"type": "text", "text": format!("Loading {REMINDER_TOKEN}")}
    ]});
    let earlier = earlier_from_messages(&history)?;
    let thread = thread_body(vec![tool_result()])?;
    assert!(matches!(
        apply_lazy_sections(&thread, &rules, "", &earlier),
        LazySectionsOutcome::Unchanged
    ));
    Ok(())
}
