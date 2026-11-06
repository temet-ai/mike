use super::tests_support::{cfg, rewrote};
use super::*;
use crate::filter::{FilterAction, RuleSource};
use serde_json::json;

const REMINDER_BODY: &str = "Use tabs_context_mcp before guessing tab ids.";
const REMINDER_TOKEN: &str = "<chrome>";

fn inject_reminder_rule(id: i64) -> FilterRule {
    FilterRule {
        id,
        uuid: uuid::Uuid::new_v4(),
        kind: FilterKind::InjectReminder,
        value: "chrome reminder".to_string(),
        enabled: true,
        source: RuleSource::Ui,
        updated_at: chrono::Utc::now(),
        sort_order: 0,
        action: FilterAction::Replace,
        text: Some(REMINDER_BODY.to_string()),
        trigger_token: Some(REMINDER_TOKEN.to_string()),
        inject_text: None,
        project_path: None,
        style_override: false,
        title: None,
    }
}

#[test]
fn inject_reminder_injects_body_wrapped_per_marker_once_per_window() -> anyhow::Result<()> {
    let system_before = json!([{
        "type": "text",
        "text": "You are Claude Code.\nNo chrome heading here.",
        "cache_control": {"type": "ephemeral"},
    }]);
    let body = serde_json::to_vec(&json!({
        "system": system_before.clone(),
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "open the page"}]},
            {"role": "assistant", "content": [{"type": "text", "text": format!("Loading {REMINDER_TOKEN}")}]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "tabs"}]},
        ],
    }))?;
    let out = apply_lazy_sections(&body, &cfg(vec![inject_reminder_rule(42)]), "");
    let (v, stripped, injected, firing) = rewrote(out)?;
    assert!(injected, "marker in latest assistant turn must inject");
    assert_eq!(firing, vec![42]);
    assert_eq!(
        stripped, 0,
        "inject_reminder never strips — stripped_bytes=0"
    );

    assert_eq!(
        v["system"], system_before,
        "system prompt must be untouched"
    );

    let content = v["messages"][2]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
    assert_eq!(content.len(), 2, "tool_result + injected reminder");
    let reminder = content[1]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("reminder text must be string"))?;
    assert_eq!(
        reminder,
        format!("<chrome>\n{REMINDER_BODY}\n</chrome>"),
        "body wrapped in the marker-derived tag, no truncation"
    );
    Ok(())
}

#[test]
fn inject_reminder_in_older_turn_is_noop() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({
        "system": [{"type": "text", "text": "plain system"}],
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "q1"}]},
            {"role": "assistant", "content": [{"type": "text", "text": format!("ask {REMINDER_TOKEN}")}]},
            {"role": "user", "content": [{"type": "text", "text": "tool turn"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "later reply, no marker"}]},
            {"role": "user", "content": [{"type": "text", "text": "latest"}]},
        ],
    }))?;
    assert!(matches!(
        apply_lazy_sections(&body, &cfg(vec![inject_reminder_rule(1)]), ""),
        LazySectionsOutcome::Unchanged
    ));
    Ok(())
}

#[test]
fn inject_reminder_marker_absent_is_unchanged() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({
        "system": [{"type": "text", "text": "plain system"}],
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "q"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "no marker here"}]},
            {"role": "user", "content": [{"type": "text", "text": "next"}]},
        ],
    }))?;
    assert!(matches!(
        apply_lazy_sections(&body, &cfg(vec![inject_reminder_rule(1)]), ""),
        LazySectionsOutcome::Unchanged
    ));
    Ok(())
}

#[test]
fn inject_reminder_disabled_is_unchanged() -> anyhow::Result<()> {
    let mut rule = inject_reminder_rule(1);
    rule.enabled = false;
    let body = serde_json::to_vec(&json!({
        "system": [{"type": "text", "text": "plain system"}],
        "messages": [
            {"role": "assistant", "content": [{"type": "text", "text": format!("x {REMINDER_TOKEN}")}]},
            {"role": "user", "content": [{"type": "text", "text": "next"}]},
        ],
    }))?;
    assert!(matches!(
        apply_lazy_sections(&body, &cfg(vec![rule]), ""),
        LazySectionsOutcome::Unchanged
    ));
    Ok(())
}

#[test]
fn inject_reminder_thinking_blocks_present_skips_r1_guard() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({
        "system": [{"type": "text", "text": "plain system"}],
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "q"}]},
            {"role": "assistant", "content": [
                {"type": "thinking", "thinking": "redacted", "signature": "sig=="},
                {"type": "text", "text": format!("answer {REMINDER_TOKEN}")},
            ]},
            {"role": "user", "content": [{"type": "text", "text": "next"}]},
        ],
    }))?;
    assert!(matches!(
        apply_lazy_sections(&body, &cfg(vec![inject_reminder_rule(1)]), ""),
        LazySectionsOutcome::Unchanged
    ));
    Ok(())
}

#[test]
fn inject_reminder_malformed_json_is_unchanged() {
    assert!(matches!(
        apply_lazy_sections(b"not json {{{", &cfg(vec![inject_reminder_rule(1)]), ""),
        LazySectionsOutcome::Unchanged
    ));
}

#[test]
fn inject_reminder_non_tag_marker_uses_default_wrapper() -> anyhow::Result<()> {
    let mut rule = inject_reminder_rule(1);
    rule.trigger_token = Some("!!REMIND!!".to_string());
    let body = serde_json::to_vec(&json!({
        "system": [{"type": "text", "text": "plain system"}],
        "messages": [
            {"role": "assistant", "content": [{"type": "text", "text": "go !!REMIND!!"}]},
            {"role": "user", "content": [{"type": "text", "text": "next"}]},
        ],
    }))?;
    let (v, _, injected, _) = rewrote(apply_lazy_sections(&body, &cfg(vec![rule]), ""))?;
    assert!(injected);
    let reminder = v["messages"][1]["content"][1]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("reminder text must be string"))?;
    assert_eq!(
        reminder,
        format!("<system-reminder>\n{REMINDER_BODY}\n</system-reminder>")
    );
    Ok(())
}
