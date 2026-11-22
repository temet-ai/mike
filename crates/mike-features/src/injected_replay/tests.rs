use super::*;
use crate::reminders::appender::append_block_to_last_user_message;
use crate::store::models::RequestId;

const LANE: &str = "session-a:primary";
const LESSON: &str = "<system-reminder source=\"proxy-lessons\">\nlesson one\n</system-reminder>";

fn first_turn() -> Vec<u8> {
    br#"{"model":"m","system":[{"type":"text","text":"s"}],"messages":[{"role":"user","content":[{"type":"text","text":"hello"},{"type":"text","text":"do it","cache_control":{"type":"ephemeral"}}]}]}"#.to_vec()
}

fn second_turn() -> Vec<u8> {
    br#"{"model":"m","system":[{"type":"text","text":"s"}],"messages":[{"role":"user","content":[{"type":"text","text":"hello"},{"type":"text","text":"do it"}]},{"role":"assistant","content":[{"type":"text","text":"done"}]},{"role":"user","content":[{"type":"text","text":"next","cache_control":{"type":"ephemeral"}}]}]}"#.to_vec()
}

fn inject(body: &[u8]) -> Vec<u8> {
    append_block_to_last_user_message(body, LESSON, RequestId::next()).unwrap_or_default()
}

fn replayed(registry: &InjectedBlockRegistry, lane: &str, body: &[u8]) -> (Vec<u8>, ReplayReport) {
    let Some((out, report)) = registry.replay(lane, body) else {
        panic!("lane {lane} has records");
    };
    (out.to_vec(), report)
}

fn message_bytes(body: &[u8], index: usize) -> String {
    let value: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let mut message = value["messages"][index].clone();
    for item in message["content"].as_array_mut().into_iter().flatten() {
        item.as_object_mut().map(|o| o.remove("cache_control"));
    }
    message.to_string()
}

fn content_tail_after(body: &[u8], needle: &str) -> String {
    let text = String::from_utf8_lossy(body).into_owned();
    let at = text.find(needle).unwrap_or_default();
    text[at..].to_string()
}

#[test]
fn replay_second_request_splices_block_bytes_identical_to_first() {
    let registry = InjectedBlockRegistry::default();
    let first = first_turn();
    let first_sent = inject(&first);
    assert!(registry.record(LANE, &first, &first_sent));

    let (second_sent, report) = replayed(&registry, LANE, &second_turn());

    assert_eq!(report.replayed_indices, vec![0]);
    assert_eq!(report.held, 1);
    assert_eq!(
        message_bytes(&second_sent, 0),
        message_bytes(&first_sent, 0)
    );
    let first_tail = content_tail_after(&first_sent, "},{\"text\":\"<system-reminder");
    let second_tail = content_tail_after(&second_sent, "},{\"text\":\"<system-reminder");
    let block_end = first_tail.find("]}").unwrap_or_default() + 2;
    assert_eq!(&second_tail[..block_end], &first_tail[..block_end]);
}

#[test]
fn replay_plain_string_message_rebuilds_the_appenders_wrapping() {
    let registry = InjectedBlockRegistry::default();
    let first = br#"{"messages":[{"role":"user","content":"hi"}]}"#.to_vec();
    let first_sent = inject(&first);
    assert!(registry.record(LANE, &first, &first_sent));
    let second =
        br#"{"messages":[{"role":"user","content":"hi"},{"role":"assistant","content":"yo"},{"role":"user","content":"more"}]}"#;

    let (second_sent, _) = replayed(&registry, LANE, second);

    let prefix = &first_sent[..first_sent.len() - 2];
    assert!(second_sent.starts_with(prefix));
}

#[test]
fn replay_changed_message_leaves_body_alone_and_keeps_record() {
    let registry = InjectedBlockRegistry::default();
    let first = first_turn();
    assert!(registry.record(LANE, &first, &inject(&first)));
    let changed = String::from_utf8_lossy(&second_turn())
        .replace("\"hello\"", "\"hello again\"")
        .into_bytes();

    let (out, report) = replayed(&registry, LANE, &changed);

    assert_eq!(out.as_slice(), changed.as_slice());
    assert!(report.replayed_indices.is_empty());
    assert_eq!(registry.records_in(LANE), 1);
}

fn continuation(text: &str) -> Vec<u8> {
    format!(
        r#"{{"thread":{{"type":"continue"}},"messages":[{{"role":"user","content":[{{"type":"text","text":"{text}","cache_control":{{"type":"ephemeral","ttl":"1h"}}}}]}}]}}"#
    )
    .into_bytes()
}

#[test]
fn replay_full_resend_after_continuations_restores_every_block_where_it_was_sent() {
    let registry = InjectedBlockRegistry::default();
    let mut sent_turns = Vec::new();
    for text in ["turn one", "turn two"] {
        let body = continuation(text);
        assert!(
            registry
                .replay(LANE, &body)
                .is_none_or(|(_, r)| r.replayed_indices.is_empty())
        );
        let sent = inject(&body);
        assert!(registry.record(LANE, &body, &sent));
        sent_turns.push(sent);
    }
    let resend = br#"{"thread":{"type":"create"},"messages":[{"role":"user","content":"turn one"},{"role":"assistant","content":[{"type":"text","text":"a"}]},{"role":"user","content":"turn two"},{"role":"assistant","content":[{"type":"text","text":"b"}]},{"role":"user","content":[{"type":"text","text":"turn three"}]}]}"#;

    let (out, report) = replayed(&registry, LANE, resend);

    assert_eq!(report.replayed_indices, vec![0, 2]);
    let value: serde_json::Value = serde_json::from_slice(&out).unwrap_or_default();
    for (index, sent) in [(0, &sent_turns[0]), (2, &sent_turns[1])] {
        let was: serde_json::Value = serde_json::from_slice(sent).unwrap_or_default();
        let mut expected = was["messages"][0]["content"].clone();
        expected[0]
            .as_object_mut()
            .map(|o| o.remove("cache_control"));
        assert_eq!(value["messages"][index]["content"], expected);
    }
    assert_eq!(value["messages"][4]["content"][0]["text"], "turn three");
}

#[test]
fn replay_after_compaction_leaves_body_alone() {
    let registry = InjectedBlockRegistry::default();
    let first = second_turn();
    let first_sent = inject(&first);
    assert!(registry.record(LANE, &first, &first_sent));
    let compacted =
        br#"{"messages":[{"role":"user","content":[{"type":"text","text":"summary"}]}]}"#;

    let (out, report) = replayed(&registry, LANE, compacted);

    assert_eq!(out.as_slice(), compacted.as_slice());
    assert!(report.replayed_indices.is_empty());
}

#[test]
fn record_unchanged_body_captures_nothing() {
    let registry = InjectedBlockRegistry::default();
    let body = first_turn();
    assert!(!registry.record(LANE, &body, &body));
    assert!(registry.replay(LANE, &body).is_none());
}

#[test]
fn replay_other_lane_sees_no_records() {
    let registry = InjectedBlockRegistry::default();
    let first = first_turn();
    assert!(registry.record(LANE, &first, &inject(&first)));
    assert!(
        registry
            .replay("session-a:agent:x", &second_turn())
            .is_none()
    );
}

#[test]
fn record_past_lane_capacity_evicts_oldest_lane() {
    let registry = InjectedBlockRegistry::with_capacity(1);
    let first = first_turn();
    let sent = inject(&first);
    assert!(registry.record("a", &first, &sent));
    assert!(registry.record("b", &first, &sent));
    assert_eq!(registry.records_in("a"), 0);
    assert_eq!(registry.records_in("b"), 1);
}

#[test]
fn record_same_message_twice_keeps_one_record() {
    let registry = InjectedBlockRegistry::default();
    let first = first_turn();
    let sent = inject(&first);
    assert!(registry.record(LANE, &first, &sent));
    assert!(registry.record(LANE, &first, &inject(&first)));
    assert_eq!(registry.records_in(LANE), 1);
}

#[test]
fn restore_from_stored_fields_replays_like_the_original_after_a_restart() {
    let before_restart = InjectedBlockRegistry::default();
    let first = first_turn();
    let first_sent = inject(&first);
    let Some(captured) = before_restart.capture(LANE, &first, &first_sent) else {
        panic!("the injected block is captured");
    };
    let Some(rebuilt) =
        splice::InjectedBlocks::restored(captured.identity, captured.tail().to_vec())
    else {
        panic!("a stored record rebuilds");
    };
    let after_restart = InjectedBlockRegistry::default();
    after_restart.restore(LANE, rebuilt);

    let (expected, _) = replayed(&before_restart, LANE, &second_turn());
    let (got, report) = replayed(&after_restart, LANE, &second_turn());

    assert_eq!(report.replayed_indices, vec![0]);
    assert_eq!(got, expected);
}

#[test]
fn take_replayed_lanes_names_each_lane_that_replayed_once() {
    let registry = InjectedBlockRegistry::default();
    let first = first_turn();
    assert!(registry.record(LANE, &first, &inject(&first)));
    assert!(registry.take_replayed_lanes().is_empty(), "recording is not replaying");

    let _ = replayed(&registry, LANE, &second_turn());
    let _ = replayed(&registry, LANE, &second_turn());
    assert_eq!(registry.take_replayed_lanes(), vec![LANE.to_owned()]);
    assert!(registry.take_replayed_lanes().is_empty(), "drained");
}

#[test]
fn take_replayed_lanes_skips_a_lane_whose_blocks_are_gone() {
    let registry = InjectedBlockRegistry::default();
    let first = first_turn();
    assert!(registry.record(LANE, &first, &inject(&first)));
    let compacted = br#"{"model":"m","system":[{"type":"text","text":"s"}],"messages":[{"role":"user","content":[{"type":"text","text":"summary"}]}]}"#;
    let _ = registry.replay(LANE, compacted);
    assert!(registry.take_replayed_lanes().is_empty());
}
