use super::tests_support::{cfg, rewrote};
use super::*;
use crate::filter::{FilterAction, RuleSource};
use serde_json::json;

const HEADING: &str = "# Claude in Chrome browser automation";
const STUB: &str = "# Claude in Chrome browser automation\n\nOn-demand: output !!CHROME!! then call tabs_context_mcp.";
const TOKEN: &str = "!!CHROME!!";

fn lazy_rule(id: i64) -> FilterRule {
    FilterRule {
        id,
        uuid: uuid::Uuid::new_v4(),
        kind: FilterKind::LazySection,
        value: HEADING.to_string(),
        enabled: true,
        source: RuleSource::Ui,
        updated_at: chrono::Utc::now(),
        sort_order: 0,
        action: FilterAction::Replace,
        text: Some(STUB.to_string()),
        trigger_token: Some(TOKEN.to_string()),
        inject_text: None,
        project_path: None,
        style_override: false,
        title: None,
    }
}

fn chrome_system_block() -> Value {
    json!({
        "type": "text",
        "text": format!(
            "You are Claude Code.\n\n{HEADING}\nUse tabs_context_mcp first.\nNever guess tab ids.\n# Tone and style\nBe concise."
        ),
        "cache_control": {"type": "ephemeral"},
    })
}

#[test]
fn strip_replaces_section_preserving_cache_control_and_siblings() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({
        "model": "m",
        "system": [
            {"type": "text", "text": "billing header"},
            chrome_system_block(),
        ],
        "messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}],
    }))?;
    let (v, stripped, injected, firing) =
        rewrote(apply_lazy_sections(&body, &cfg(vec![lazy_rule(7)]), ""))?;
    let arr = v["system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("system must be array"))?;
    assert_eq!(arr.len(), 2, "sibling block survives");
    assert_eq!(arr[0]["text"], "billing header");
    let text = arr[1]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("text must be string"))?;
    assert!(text.contains(STUB), "stub replaces the section");
    assert!(
        !text.contains("Never guess tab ids."),
        "section body stripped"
    );
    assert!(
        text.contains("# Tone and style\nBe concise."),
        "following section untouched"
    );
    assert!(text.starts_with("You are Claude Code."), "prefix untouched");
    assert_eq!(
        arr[1]["cache_control"]["type"], "ephemeral",
        "cache_control preserved on the stripped block"
    );
    assert!(stripped > 0);
    assert!(!injected, "no trigger in history yet");
    assert_eq!(firing, vec![7]);
    Ok(())
}

#[test]
fn strip_skips_a_globally_scoped_block() -> anyhow::Result<()> {
    let mut chrome = chrome_system_block();
    chrome["cache_control"] = json!({"type": "ephemeral", "scope": "global"});
    let body = serde_json::to_vec(&json!({
        "model": "m",
        "system": [{"type": "text", "text": "billing header"}, chrome],
        "messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}],
    }))?;
    match apply_lazy_sections(&body, &cfg(vec![lazy_rule(7)]), "") {
        LazySectionsOutcome::Unchanged => Ok(()),
        LazySectionsOutcome::Rewrote { .. } => Err(anyhow::anyhow!(
            "expected Unchanged — a globally-scoped block must never be stripped"
        )),
    }
}

#[test]
fn trigger_in_history_strips_and_injects() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({
        "system": [chrome_system_block()],
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "open the page"}]},
            {"role": "assistant", "content": [{"type": "text", "text": format!("Loading guidance: {TOKEN}")}]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "tabs"}]},
        ],
    }))?;
    let (v, _, injected, _) = rewrote(apply_lazy_sections(&body, &cfg(vec![lazy_rule(1)]), ""))?;
    assert!(injected);
    let content = v["messages"][2]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
    assert_eq!(content.len(), 2);
    let reminder = content[1]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("reminder text must be string"))?;
    assert!(reminder.starts_with("<system-reminder>\n"));
    assert!(reminder.ends_with("\n</system-reminder>"));
    assert!(
        reminder.contains("Never guess tab ids."),
        "FULL section text injected — no truncation"
    );
    Ok(())
}

#[test]
fn tag_marker_wraps_reminder_in_matching_tag() -> anyhow::Result<()> {
    let mut rule = lazy_rule(1);
    rule.trigger_token = Some("<chrome>".to_string());
    let body = serde_json::to_vec(&json!({
        "system": [chrome_system_block()],
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "open the page"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "Loading guidance: <chrome>"}]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "tabs"}]},
        ],
    }))?;
    let (v, _, injected, _) = rewrote(apply_lazy_sections(&body, &cfg(vec![rule]), ""))?;
    assert!(injected);
    let reminder = v["messages"][2]["content"][1]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("reminder text must be string"))?;
    assert!(reminder.starts_with("<chrome>\n"), "got: {reminder}");
    assert!(reminder.ends_with("\n</chrome>"), "got: {reminder}");
    assert!(!reminder.contains("<system-reminder>"));
    Ok(())
}

#[test]
fn bare_tag_name_extracts_simple_tags() {
    assert_eq!(bare_tag_name("<chrome>"), Some("chrome"));
    assert_eq!(bare_tag_name("  <my-tag_2> "), Some("my-tag_2"));
}

#[test]
fn bare_tag_name_rejects_non_tag_markers() {
    for non_tag in ["!!CHROME!!", "<>", "<tag attr>", "<a/b>", "chrome", "<a>b"] {
        assert_eq!(
            bare_tag_name(non_tag),
            None,
            "{non_tag:?} is not a bare tag"
        );
    }
}

#[test]
fn block_that_is_only_the_section_with_whitespace_stub_is_dropped() -> anyhow::Result<()> {
    let mut rule = lazy_rule(1);
    rule.text = Some(" ".to_string());
    let body = serde_json::to_vec(&json!({
        "system": [
            {"type": "text", "text": format!("{HEADING}\nonly the section")},
            {"type": "text", "text": "survivor"},
        ],
        "messages": [],
    }))?;
    let (v, _, _, _) = rewrote(apply_lazy_sections(&body, &cfg(vec![rule]), ""))?;
    let arr = v["system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("system must be array"))?;
    assert_eq!(arr.len(), 1, "hollowed block dropped");
    assert_eq!(arr[0]["text"], "survivor");
    Ok(())
}

#[test]
fn heading_absent_is_unchanged() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({
        "system": [{"type": "text", "text": "no chrome section here"}],
        "messages": [],
    }))?;
    assert!(matches!(
        apply_lazy_sections(&body, &cfg(vec![lazy_rule(1)]), ""),
        LazySectionsOutcome::Unchanged
    ));
    Ok(())
}

#[test]
fn disabled_rule_is_unchanged() -> anyhow::Result<()> {
    let mut rule = lazy_rule(1);
    rule.enabled = false;
    let body = serde_json::to_vec(&json!({
        "system": [chrome_system_block()],
        "messages": [],
    }))?;
    assert!(matches!(
        apply_lazy_sections(&body, &cfg(vec![rule]), ""),
        LazySectionsOutcome::Unchanged
    ));
    Ok(())
}

#[test]
fn non_lazy_rules_are_ignored() -> anyhow::Result<()> {
    let other = FilterRule {
        id: 9,
        uuid: uuid::Uuid::new_v4(),
        kind: FilterKind::TextSubstring,
        value: "You are Claude Code.".to_string(),
        enabled: true,
        source: RuleSource::Ui,
        updated_at: chrono::Utc::now(),
        sort_order: 0,
        action: FilterAction::Drop,
        text: None,
        trigger_token: None,
        inject_text: None,
        project_path: None,
        style_override: false,
        title: None,
    };
    let body = serde_json::to_vec(&json!({
        "system": [chrome_system_block()],
        "messages": [],
    }))?;
    assert!(matches!(
        apply_lazy_sections(&body, &cfg(vec![other]), ""),
        LazySectionsOutcome::Unchanged
    ));
    Ok(())
}

#[test]
fn string_shaped_system_is_never_touched() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({
        "system": format!("{HEADING}\nstring-shaped"),
        "messages": [],
    }))?;
    assert!(matches!(
        apply_lazy_sections(&body, &cfg(vec![lazy_rule(1)]), ""),
        LazySectionsOutcome::Unchanged
    ));
    Ok(())
}

#[test]
fn malformed_json_is_unchanged() {
    assert!(matches!(
        apply_lazy_sections(b"not json {{{", &cfg(vec![lazy_rule(1)]), ""),
        LazySectionsOutcome::Unchanged
    ));
}

#[test]
fn thinking_blocks_present_skips_rewrite_r1_guard() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({
        "system": [chrome_system_block()],
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "q"}]},
            {"role": "assistant", "content": [
                {"type": "thinking", "thinking": "…", "signature": "sig=="},
                {"type": "text", "text": format!("answer {TOKEN}")},
            ]},
            {"role": "user", "content": [{"type": "text", "text": "next"}]},
        ],
    }))?;
    assert!(matches!(
        apply_lazy_sections(&body, &cfg(vec![lazy_rule(1)]), ""),
        LazySectionsOutcome::Unchanged
    ));
    Ok(())
}

#[test]
fn project_scoped_rule_for_other_project_is_unchanged() -> anyhow::Result<()> {
    let mut rule = lazy_rule(1);
    rule.project_path = Some("projects/temet/other".to_string());
    let body = serde_json::to_vec(&json!({
        "system": [chrome_system_block()],
        "messages": [],
    }))?;
    assert!(matches!(
        apply_lazy_sections(&body, &cfg(vec![rule]), "projects/temet/temet-proxy"),
        LazySectionsOutcome::Unchanged
    ));
    Ok(())
}

#[test]
fn strip_is_idempotent_over_the_stub() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({
        "system": [chrome_system_block()],
        "messages": [],
    }))?;
    let c = cfg(vec![lazy_rule(1)]);
    let (first, _, _, _) = rewrote(apply_lazy_sections(&body, &c, ""))?;
    let first_bytes = serde_json::to_vec(&first)?;
    let (second, _, _, _) = rewrote(apply_lazy_sections(&first_bytes, &c, ""))?;
    assert_eq!(first, second, "second pass must be a fixpoint");
    Ok(())
}
