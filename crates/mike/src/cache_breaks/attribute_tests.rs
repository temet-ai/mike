use super::*;

fn body(system: &str, tools: &[&str], messages: &[&str]) -> Value {
    json!({
        "system": [
            {"type": "text", "text": "x-anthropic-billing-header: cch=abc"},
            {"type": "text", "text": system, "cache_control": {"type": "ephemeral", "ttl": "1h"}}
        ],
        "tools": tools.iter().map(|n| json!({"name": n})).collect::<Vec<_>>(),
        "messages": messages.iter().map(|m| json!({"role": "user", "content": m})).collect::<Vec<_>>()
    })
}

fn delta(messages: &[&str]) -> Value {
    json!({
        "thread": {"type": "continue", "previous_message_id": "msg_1"},
        "messages": messages.iter().map(|m| json!({"role": "user", "content": m})).collect::<Vec<_>>()
    })
}

fn sent(id: i64, sent: Value, original: Value, applied: &[&str], filter: &str) -> Sent {
    Sent {
        request_id: id,
        sent,
        original,
        stages: Stages {
            applied: applied.iter().map(|s| (*s).to_owned()).collect(),
            filter_reason: Some(filter.to_owned()),
        },
    }
}

fn plain(id: i64, b: Value) -> Sent {
    sent(id, b.clone(), b, &[], "filter_replay_applied")
}

#[test]
fn attribute_identical_resend_names_no_cause() {
    let full = plain(1, body("sys", &["Read"], &["a"]));
    let cur = plain(3, body("sys", &["Read"], &["a", "b", "c"]));
    let chain = Chain {
        full: Some(full),
        deltas: vec![plain(2, delta(&["b"]))],
    };
    let a = attribute(&cur, &chain);
    assert_eq!((a.cause, a.location), (Cause::Unknown, "none"));
}

#[test]
fn attribute_billing_header_and_cache_control_changes_are_ignored() {
    let mut other = body("sys", &["Read"], &["a", "b"]);
    other["system"][0]["text"] = json!("x-anthropic-billing-header: cch=zzz");
    other["system"][1]["cache_control"] = json!({"type": "ephemeral"});
    let full = plain(1, body("sys", &["Read"], &["a"]));
    let a = attribute(
        &plain(2, other),
        &Chain {
            full: Some(full),
            deltas: vec![],
        },
    );
    assert_eq!(a.cause, Cause::Unknown);
}

#[test]
fn attribute_block_injected_on_a_continuation_and_not_replayed_is_that_stage() {
    let full = plain(1, body("sys", &["Read"], &["a"]));
    let d = sent(
        2,
        delta(&["b + identity"]),
        delta(&["b"]),
        &["self_identity"],
        "filter_replay_applied",
    );
    let cur = plain(3, body("sys", &["Read"], &["a", "b", "c"]));
    let a = attribute(
        &cur,
        &Chain {
            full: Some(full),
            deltas: vec![d],
        },
    );
    assert_eq!(
        (a.cause, a.stage.as_deref(), a.location, a.source_request_id),
        (Cause::Mike, Some("self_identity"), "delta", Some(2))
    );
}

#[test]
fn attribute_filter_flip_between_full_sends_is_the_filter() {
    let original = body("long system", &["Read"], &["a"]);
    let full = sent(
        1,
        body("short system", &["Read"], &["a"]),
        original,
        &["filter"],
        "filter_replay_applied",
    );
    let cur_original = body("long system", &["Read"], &["a", "b"]);
    let cur = sent(
        2,
        cur_original.clone(),
        cur_original,
        &[],
        "filter_replay_system_mismatch",
    );
    let a = attribute(
        &cur,
        &Chain {
            full: Some(full),
            deltas: vec![],
        },
    );
    assert_eq!(
        (a.cause, a.stage.as_deref(), a.location),
        (Cause::Mike, Some("filter"), "system")
    );
    assert_eq!(
        a.detail["filter_to"],
        json!("filter_replay_system_mismatch")
    );
}

#[test]
fn attribute_claude_code_adding_tools_is_the_client() {
    let full = plain(1, body("sys", &["Read"], &["a"]));
    let cur = plain(
        2,
        body("sys", &["Read", "mcp__mike__get_request"], &["a", "b"]),
    );
    let a = attribute(
        &cur,
        &Chain {
            full: Some(full),
            deltas: vec![],
        },
    );
    assert_eq!((a.cause, a.location), (Cause::Client, "tools"));
    assert_eq!(a.detail["tools_added"], json!(["mcp__mike__get_request"]));
}

#[test]
fn attribute_lessons_on_the_full_send_dropped_later_is_session_lessons() {
    let full = sent(
        1,
        body("sys", &["Read"], &["a + lessons"]),
        body("sys", &["Read"], &["a"]),
        &["filter", "session_lessons"],
        "filter_replay_applied",
    );
    let cur = plain(2, body("sys", &["Read"], &["a", "b"]));
    let a = attribute(
        &cur,
        &Chain {
            full: Some(full),
            deltas: vec![],
        },
    );
    assert_eq!(
        (a.cause, a.stage.as_deref(), a.location),
        (Cause::Mike, Some("session_lessons"), "messages_prefix")
    );
}

#[test]
fn attribute_client_rewriting_history_is_the_client() {
    let full = plain(1, body("sys", &["Read"], &["a", "b"]));
    let cur = plain(2, body("sys", &["Read"], &["summary", "c"]));
    let a = attribute(
        &cur,
        &Chain {
            full: Some(full),
            deltas: vec![],
        },
    );
    assert_eq!((a.cause, a.location), (Cause::Client, "messages_prefix"));
}

#[test]
fn attribute_continuation_and_broken_chain_are_unknown() {
    let cur = plain(2, delta(&["b"]));
    assert_eq!(attribute(&cur, &Chain::default()).location, "continuation");
    let cur = plain(2, body("sys", &[], &["a"]));
    assert_eq!(attribute(&cur, &Chain::default()).location, "chain");
}
