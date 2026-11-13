use super::content_blocks::transform_body;
use super::content_blocks_test_support::{body_bytes, cfg, rewrote, rule, rule_with_id};
use super::{FilterAction, FilterKind};
use serde_json::json;

#[test]
fn slice_drop_removes_matched_slice_only() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "system": [{
            "type": "text",
            "text": "Keep this. REMOVE ME. Keep that.",
            "cache_control": {"type": "ephemeral"},
        }]
    }))?;
    let c = cfg(vec![rule(
        "slice-drop",
        FilterKind::TextSubstring,
        "remove me. ",
        FilterAction::Drop,
        None,
    )]);
    let (v, _) = rewrote(transform_body(&body, &c, ""))?;
    let block = &v["system"][0];
    assert_eq!(block["text"], "Keep this. Keep that.");
    assert_eq!(block["cache_control"]["type"], "ephemeral");
    Ok(())
}

#[test]
fn slice_replace_swaps_first_occurrence_only() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "system": [{"type": "text", "text": "alpha beta ALPHA"}]
    }))?;
    let c = cfg(vec![rule(
        "slice-replace",
        FilterKind::TextSubstring,
        "Alpha",
        FilterAction::Replace,
        Some("gamma"),
    )]);
    let (v, _) = rewrote(transform_body(&body, &c, ""))?;
    assert_eq!(v["system"][0]["text"], "gamma beta ALPHA");
    Ok(())
}

#[test]
fn slice_append_inserts_text_after_slice_inside_block() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "system": [{"type": "text", "text": "Header: rest of prose."}]
    }))?;
    let c = cfg(vec![rule(
        "slice-append",
        FilterKind::TextSubstring,
        "header:",
        FilterAction::Append,
        Some(" [annotated]"),
    )]);
    let (v, _) = rewrote(transform_body(&body, &c, ""))?;
    let arr = v["system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("system must be array"))?;
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["text"], "Header: [annotated] rest of prose.");
    Ok(())
}

#[test]
fn multiple_narrowed_rules_compose_on_one_block() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "system": [{"type": "text", "text": "line one. line two. line three."}]
    }))?;
    let c = cfg(vec![
        rule_with_id(
            1,
            FilterKind::TextSubstring,
            "line one. ",
            FilterAction::Drop,
            None,
        ),
        rule_with_id(
            2,
            FilterKind::TextSubstring,
            "line two.",
            FilterAction::Replace,
            Some("LINE 2."),
        ),
        rule_with_id(
            3,
            FilterKind::TextSubstring,
            "line three.",
            FilterAction::Append,
            Some(" END"),
        ),
    ]);
    let (v, firing) = rewrote(transform_body(&body, &c, ""))?;
    assert_eq!(v["system"][0]["text"], "LINE 2. line three. END");
    assert_eq!(
        firing,
        vec![1, 2, 3],
        "every narrowed rule fired, in rule order"
    );
    Ok(())
}

#[test]
fn whole_block_drop_short_circuits_remaining_rules() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "system": [{"type": "text", "text": "<system-reminder>noise</system-reminder>"}]
    }))?;
    let c = cfg(vec![
        rule_with_id(
            10,
            FilterKind::XmlTag,
            "system-reminder",
            FilterAction::Drop,
            None,
        ),
        rule_with_id(
            11,
            FilterKind::TextSubstring,
            "noise",
            FilterAction::Append,
            Some("must not appear"),
        ),
    ]);
    let (v, firing) = rewrote(transform_body(&body, &c, ""))?;
    let arr = v["system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("system must be array"))?;
    assert!(
        arr.is_empty(),
        "whole-block drop removes the block; no cascade"
    );
    assert_eq!(
        firing,
        vec![10],
        "short-circuited rule must not be recorded"
    );
    Ok(())
}

#[test]
fn whole_block_replace_short_circuits_remaining_rules() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "system": [{"type": "text", "text": "Original Content"}]
    }))?;
    let c = cfg(vec![
        rule_with_id(
            20,
            FilterKind::TextSubstring,
            "original content",
            FilterAction::Replace,
            Some("fresh content"),
        ),
        rule_with_id(
            21,
            FilterKind::TextSubstring,
            "fresh",
            FilterAction::Append,
            Some(" MORE"),
        ),
    ]);
    let (v, firing) = rewrote(transform_body(&body, &c, ""))?;
    assert_eq!(
        v["system"][0]["text"], "fresh content",
        "whole-block replace short-circuits; later append must not fire"
    );
    assert_eq!(firing, vec![20]);
    Ok(())
}

#[test]
fn slice_offsets_are_correct_with_case_insensitive_non_ascii_text() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "system": [{"type": "text", "text": "Übergröße: STRAẞE prose"}]
    }))?;
    let c = cfg(vec![rule(
        "non-ascii",
        FilterKind::TextSubstring,
        "straße ",
        FilterAction::Drop,
        None,
    )]);
    let (v, _) = rewrote(transform_body(&body, &c, ""))?;
    assert_eq!(v["system"][0]["text"], "Übergröße: prose");
    Ok(())
}
