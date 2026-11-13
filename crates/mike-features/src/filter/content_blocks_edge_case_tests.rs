use super::content_blocks::transform_body;
use super::content_blocks_test_support::{body_bytes, cfg, rewrote, rule, rule_with_id};
use super::{FilterAction, FilterKind, FilterOutcome};
use serde_json::json;

#[test]
fn block_emptied_by_slice_drops_is_dropped_entirely() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "system": [
            {"type": "text", "text": "alpha beta"},
            {"type": "text", "text": "survivor"}
        ]
    }))?;
    let c = cfg(vec![
        rule_with_id(
            30,
            FilterKind::TextSubstring,
            "alpha ",
            FilterAction::Drop,
            None,
        ),
        rule_with_id(
            31,
            FilterKind::TextSubstring,
            "beta",
            FilterAction::Drop,
            None,
        ),
    ]);
    let (v, firing) = rewrote(transform_body(&body, &c, ""))?;
    let arr = v["system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("system must be array"))?;
    assert_eq!(arr.len(), 1, "emptied block dropped");
    assert_eq!(arr[0]["text"], "survivor");
    assert_eq!(firing, vec![30, 31]);
    Ok(())
}

#[test]
fn block_left_whitespace_only_by_slice_drop_is_dropped() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "system": [{"type": "text", "text": "  REMOVE ME  "}]
    }))?;
    let c = cfg(vec![rule(
        "ws-drop",
        FilterKind::TextSubstring,
        "remove me",
        FilterAction::Drop,
        None,
    )]);
    let (v, _) = rewrote(transform_body(&body, &c, ""))?;
    let arr = v["system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("system must be array"))?;
    assert!(arr.is_empty(), "whitespace-only husk dropped");
    Ok(())
}

#[test]
fn string_system_is_never_touched() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "system": "<system-reminder>strip me</system-reminder>",
        "messages": []
    }))?;
    let c = cfg(vec![rule(
        "drop",
        FilterKind::XmlTag,
        "system-reminder",
        FilterAction::Drop,
        None,
    )]);
    assert!(matches!(
        transform_body(&body, &c, ""),
        FilterOutcome::Unchanged
    ));
    Ok(())
}

#[test]
fn non_text_blocks_passthrough() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "messages": [{
            "role": "user",
            "content": [
                {"type": "tool_result", "tool_use_id": "abc", "content": "out"},
                {"type": "text", "text": "<system-reminder>drop</system-reminder>"}
            ]
        }]
    }))?;
    let c = cfg(vec![rule(
        "drop",
        FilterKind::XmlTag,
        "system-reminder",
        FilterAction::Drop,
        None,
    )]);
    let (v, _) = rewrote(transform_body(&body, &c, ""))?;
    let arr = v["messages"][0]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["type"], "tool_result");
    assert_eq!(arr[0]["tool_use_id"], "abc");
    Ok(())
}

#[test]
fn malformed_json_returns_unchanged() {
    let c = cfg(vec![rule(
        "drop",
        FilterKind::XmlTag,
        "x",
        FilterAction::Drop,
        None,
    )]);
    assert!(matches!(
        transform_body(b"not json {{{", &c, ""),
        FilterOutcome::Unchanged
    ));
}

#[test]
fn no_matching_block_returns_unchanged() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "system": [{"type": "text", "text": "plain content"}]
    }))?;
    let c = cfg(vec![rule(
        "drop",
        FilterKind::XmlTag,
        "system-reminder",
        FilterAction::Drop,
        None,
    )]);
    assert!(matches!(
        transform_body(&body, &c, ""),
        FilterOutcome::Unchanged
    ));
    Ok(())
}

#[test]
fn replace_with_none_text_keeps_block_untouched_and_warns() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "system": [{
            "type": "text",
            "text": "The following skills are available in this session.",
            "cache_control": {"type": "ephemeral"},
        }]
    }))?;
    let bad = rule(
        "bad-replace",
        FilterKind::TextSubstring,
        "skills are available",
        FilterAction::Replace,
        None,
    );
    let c = cfg(vec![bad]);
    match transform_body(&body, &c, "") {
        FilterOutcome::Unchanged => Ok(()),
        FilterOutcome::Rewrote { .. } => Err(anyhow::anyhow!(
            "expected Unchanged when Replace rule has text=None"
        )),
    }
}

#[test]
fn append_with_none_text_keeps_block_untouched_and_warns() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "system": [{
            "type": "text",
            "text": "You are an interactive agent running in the user's terminal.",
            "cache_control": {"type": "ephemeral"},
        }]
    }))?;
    let bad = rule(
        "bad-append",
        FilterKind::TextSubstring,
        "interactive agent",
        FilterAction::Append,
        None,
    );
    let c = cfg(vec![bad]);
    match transform_body(&body, &c, "") {
        FilterOutcome::Unchanged => Ok(()),
        FilterOutcome::Rewrote { .. } => Err(anyhow::anyhow!(
            "expected Unchanged when Append rule has text=None"
        )),
    }
}
