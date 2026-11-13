#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test fixtures build their own JSON bodies; a malformed fixture must fail loudly"
)]

use super::*;

fn body(billing: &str, mutable: &str) -> Vec<u8> {
    format!(
        r#"{{"model":"claude-opus-5","messages":[{{"role":"user","content":"hi"}}],"system":[{{"type":"text","text":"{billing}"}},{{"type":"text","text":"You are Claude Code."}},{{"cache_control":{{"scope":"global","ttl":"1h","type":"ephemeral"}},"type":"text","text":"main"}},{{"cache_control":{{"ttl":"1h","type":"ephemeral"}},"type":"text","text":"{mutable}"}}]}}"#
    )
    .into_bytes()
}

#[test]
fn region_starts_after_the_last_globally_scoped_block() {
    let raw = body("cch=aaa", "project");
    let region = locate_mutable_region(&raw).expect("region");
    assert_eq!(region.boundary, Some(2));
    let text = std::str::from_utf8(&raw[region.start..region.end]).unwrap();
    assert!(
        text.starts_with(",{"),
        "region must open with the separating comma; got {text}"
    );
    assert!(text.contains("project"));
    assert!(
        !text.contains("cch=aaa") && !text.contains(r#""main""#),
        "protected prefix must be outside the region; got {text}"
    );
}

#[test]
fn replay_rewrites_a_later_request_and_leaves_the_billing_header_live() {
    let first = body("cch=aaa", "project");
    let filtered = body("cch=aaa", "project APPENDED");
    let replay = SystemReplay::capture(&first, &filtered, &[62], 7).expect("capture");

    let later = body("cch=zzz", "project");
    let out = replay.apply(&later).expect("replay applies");
    let text = String::from_utf8(out.to_vec()).unwrap();

    assert!(text.contains("project APPENDED"), "rewrite must land");
    assert!(
        text.contains("cch=zzz") && !text.contains("cch=aaa"),
        "the live request's billing header must survive"
    );
    assert_eq!(replay.firing_rule_ids(), &[62]);
}

#[test]
fn replay_leaves_messages_bytes_untouched() {
    let first = body("cch=aaa", "project");
    let filtered = body("cch=aaa", "project APPENDED");
    let replay = SystemReplay::capture(&first, &filtered, &[62], 7).expect("capture");

    let later = body("cch=zzz", "project");
    let out = replay.apply(&later).expect("replay applies");

    let messages = r#""messages":[{"role":"user","content":"hi"}]"#;
    let before = std::str::from_utf8(&later).unwrap();
    let after = std::str::from_utf8(&out).unwrap();
    assert!(before.contains(messages) && after.contains(messages));
    let split = before.find(r#""system""#).unwrap();
    assert_eq!(&before[..split], &after[..split]);
}

#[test]
fn replay_skips_a_request_whose_prompt_changed() {
    let first = body("cch=aaa", "project");
    let filtered = body("cch=aaa", "project APPENDED");
    let replay = SystemReplay::capture(&first, &filtered, &[62], 7).expect("capture");

    let recomposed = body("cch=zzz", "a different project block");
    assert!(
        replay.apply(&recomposed).is_none(),
        "a re-composed prompt must forward as the client sent it"
    );
}

#[test]
fn replay_skips_a_body_with_no_system_array() {
    let first = body("cch=aaa", "project");
    let filtered = body("cch=aaa", "project APPENDED");
    let replay = SystemReplay::capture(&first, &filtered, &[62], 7).expect("capture");
    assert!(replay.apply(br#"{"model":"m","messages":[]}"#).is_none());
}

#[test]
fn capture_is_none_when_only_messages_changed() {
    let first = body("cch=aaa", "project");
    let mut messages_only = String::from_utf8(first.clone()).unwrap();
    messages_only = messages_only.replace(r#""content":"hi""#, r#""content":"rewritten""#);
    assert!(
        SystemReplay::capture(&first, messages_only.as_bytes(), &[1], 7).is_none(),
        "a messages-only rewrite has no system region to replay"
    );
}

#[test]
fn capture_handles_a_prompt_with_no_globally_scoped_block() {
    let first = br#"{"system":[{"type":"text","text":"one"}],"messages":[]}"#;
    let filtered = br#"{"system":[{"type":"text","text":"one two"}],"messages":[]}"#;
    let replay = SystemReplay::capture(first, filtered, &[7], 7).expect("capture");
    let out = replay.apply(first).expect("apply");
    assert!(String::from_utf8(out.to_vec()).unwrap().contains("one two"));
}

fn unscoped(billing: &str, mutable: &str) -> Vec<u8> {
    format!(
        r#"{{"system":[{{"type":"text","text":"x-anthropic-billing-header: cc_version=2.1.284; cch={billing};"}},{{"cache_control":{{"ttl":"1h","type":"ephemeral"}},"type":"text","text":"You are Claude Code."}},{{"cache_control":{{"ttl":"1h","type":"ephemeral"}},"type":"text","text":"{mutable}"}}],"messages":[]}}"#
    )
    .into_bytes()
}

#[test]
fn a_changing_billing_header_does_not_stop_the_replay_when_no_block_is_globally_scoped() {
    let first = unscoped("7af22", "browser section");
    let filtered = unscoped("7af22", "browser section, rewritten");
    let replay = SystemReplay::capture(&first, &filtered, &[70], 7).expect("capture");

    let later = unscoped(
        "ca751; cc_prev_req=req_011; cc_turn_index=2",
        "browser section",
    );
    let region = locate_mutable_region(&later).expect("region");
    assert_eq!(region.boundary, Some(0));
    let out = replay
        .apply(&later)
        .expect("the replay applies to a later request");
    let text = String::from_utf8(out.to_vec()).unwrap();
    assert!(text.contains("browser section, rewritten"));
    assert!(
        text.contains("cch=ca751") && !text.contains("cch=7af22"),
        "the later request keeps its own billing header"
    );
}

#[test]
fn capture_replays_a_rewrite_that_drops_every_mutable_block() {
    let first = body("cch=aaa", "project");
    let full = String::from_utf8(first.clone()).unwrap();
    let cut = full.find(r#",{"cache_control":{"ttl":"1h""#).unwrap();
    let dropped = format!("{}]}}", &full[..cut]);
    let replay = SystemReplay::capture(&first, dropped.as_bytes(), &[9], 7).expect("capture");
    let later = body("cch=zzz", "project");
    let out = replay.apply(&later).expect("apply");
    let text = String::from_utf8(out.to_vec()).unwrap();
    assert!(!text.contains("project"), "dropped block must be gone");
    assert!(text.contains("cch=zzz"), "prefix must survive");
    serde_json::from_str::<serde_json::Value>(&text).expect("result must be valid JSON");
}

#[test]
fn a_ruleset_edit_invalidates_the_fingerprint() {
    use crate::filter::{FilterAction, FilterConfig, FilterKind, FilterRule, RuleSource};
    let rule = |updated: chrono::DateTime<chrono::Utc>| FilterRule {
        id: 62,
        uuid: uuid::Uuid::nil(),
        kind: FilterKind::TextSubstring,
        value: "x".to_string(),
        enabled: true,
        source: RuleSource::Ui,
        updated_at: updated,
        sort_order: 0,
        action: FilterAction::Append,
        text: Some("y".to_string()),
        trigger_token: None,
        inject_text: None,
        project_path: None,
        style_override: false,
        title: None,
    };
    let t0 = chrono::Utc::now();
    let before = FilterConfig {
        rules: vec![rule(t0)],
    };
    let touched = FilterConfig {
        rules: vec![rule(t0 + chrono::Duration::seconds(1))],
    };
    let mut disabled_rule = rule(t0);
    disabled_rule.enabled = false;
    let disabled = FilterConfig {
        rules: vec![disabled_rule],
    };

    let base = ruleset_fingerprint(&before, "");
    assert_eq!(base, ruleset_fingerprint(&before, ""), "must be stable");
    assert_ne!(
        base,
        ruleset_fingerprint(&touched, ""),
        "an edit must differ"
    );
    assert_ne!(
        base,
        ruleset_fingerprint(&disabled, ""),
        "disabling a rule must differ"
    );
}

#[test]
fn registry_evicts_oldest_past_capacity() {
    let first = body("cch=aaa", "project");
    let filtered = body("cch=aaa", "project APPENDED");
    let replay = SystemReplay::capture(&first, &filtered, &[62], 7).expect("capture");

    let registry = ReplayRegistry::with_capacity(2);
    registry.insert("s1", replay.clone());
    registry.insert("s2", replay.clone());
    registry.insert("s3", replay.clone());

    assert_eq!(registry.len(), 2);
    assert!(!registry.has("s1"), "oldest must be evicted");
    assert!(registry.has("s2") && registry.has("s3"));
}

#[test]
fn registry_reinsert_does_not_consume_a_second_slot() {
    let first = body("cch=aaa", "project");
    let filtered = body("cch=aaa", "project APPENDED");
    let replay = SystemReplay::capture(&first, &filtered, &[62], 7).expect("capture");

    let registry = ReplayRegistry::with_capacity(2);
    registry.insert("s1", replay.clone());
    registry.insert("s1", replay.clone());
    registry.insert("s2", replay.clone());

    assert!(registry.has("s1") && registry.has("s2"));
    assert_eq!(registry.len(), 2);
}

#[test]
fn a_session_marked_unfiltered_is_unmarked_once_a_rewrite_is_captured() {
    let first = body("cch=aaa", "project");
    let filtered = body("cch=aaa", "project APPENDED");
    let replay = SystemReplay::capture(&first, &filtered, &[62], 7).expect("capture");

    let registry = ReplayRegistry::with_capacity(2);
    assert!(!registry.is_unfiltered("s1"));
    registry.mark_unfiltered("s1");
    assert!(registry.is_unfiltered("s1"));
    registry.insert("s1", replay);
    assert!(!registry.is_unfiltered("s1") && registry.has("s1"));
}
