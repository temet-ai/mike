use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};

use crate::dossier::genuine::repeat::repeat_key;
use crate::dossier::genuine::stream::{StreamMessage, resolve_turns};
use crate::dossier::genuine::{PromptInput, classify_prompt};

use super::resolve::{
    NextState, Prior, ResolveContext, TurnRow, is_reclassification, resolve_turns_for_facts,
};

fn t(minute: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(1_790_000_000, 0).unwrap_or_default()
        + Duration::minutes(minute)
}

fn message(request_id: i64, minute: i64, text: &str, turn_id: Option<i64>) -> StreamMessage {
    StreamMessage {
        request_id,
        at: t(minute),
        capture: classify_prompt(&PromptInput {
            text,
            last_user_message: None,
            author: "operator",
            has_agent_id: false,
        }),
        repeat_key: repeat_key(text),
        turn_id,
    }
}

fn turn(id: i64, start: i64, created: i64, closed: Option<i64>) -> TurnRow {
    TurnRow {
        turn_id: id,
        start_request_id: start,
        created_at: t(created),
        closed_at: closed.map(t),
        opened_by: None,
    }
}

fn ctx(stream: &[StreamMessage], last_seen: i64, now: i64) -> ResolveContext<'_> {
    ResolveContext {
        stream,
        last_seen: t(last_seen),
        now: t(now),
        settle: Duration::minutes(30),
    }
}

const LOOP_TEXT: &str = "Autonomous loop check number 7 for the coordinator seat, keep polling";
const PEER_TEXT: &str = "<teammate-message teammate_id=\"a\">hello there</teammate-message>";

#[test]
fn next_state_resolved_when_next_genuine_exists() -> Result<()> {
    let stream = vec![
        message(1, 0, "please add a retry to the client", Some(10)),
        message(2, 5, "thanks, now add the docs", Some(11)),
    ];
    let turns = vec![turn(10, 1, 0, Some(5)), turn(11, 2, 5, None)];
    let out = resolve_turns_for_facts(&ctx(&stream, 6, 7), &turns);
    assert_eq!(out.len(), 1);
    let now = &out[0].now;
    assert_eq!(now.next_state, NextState::Resolved);
    assert_eq!(now.next_genuine_request_id, Some(2));
    assert_eq!(now.next_genuine_turn_id, Some(11));
    assert_eq!(now.kind, "genuine");
    Ok(())
}

#[test]
fn next_state_pending_when_session_not_settled() {
    let stream = vec![message(1, 0, "please add a retry to the client", Some(10))];
    let turns = vec![turn(10, 1, 0, Some(1))];
    let out = resolve_turns_for_facts(&ctx(&stream, 10, 20), &turns);
    assert_eq!(out[0].now.next_state, NextState::Pending);
    assert_eq!(out[0].now.next_genuine_request_id, None);
}

#[test]
fn next_state_none_session_settled_after_settle_window() {
    let stream = vec![message(1, 0, "please add a retry to the client", Some(10))];
    let turns = vec![turn(10, 1, 0, Some(1))];
    let out = resolve_turns_for_facts(&ctx(&stream, 10, 60), &turns);
    assert_eq!(out[0].now.next_state, NextState::NoneSessionSettled);
    assert_eq!(out[0].at_close.next_state, NextState::Pending);
}

#[test]
fn at_close_resolution_ignores_messages_after_closed_at() {
    let stream = vec![
        message(1, 0, "please add a retry to the client", Some(10)),
        message(2, 30, "and then add the docs as well", Some(11)),
    ];
    let turns = vec![turn(10, 1, 0, Some(5))];
    let out = resolve_turns_for_facts(&ctx(&stream, 31, 32), &turns);
    assert_eq!(out[0].at_close.next_genuine_request_id, None);
    assert_eq!(out[0].at_close.next_state, NextState::Pending);
    assert_eq!(out[0].now.next_genuine_request_id, Some(2));
}

#[test]
fn reply_window_lists_machine_segments_between_turn_and_next_genuine() {
    let stream = vec![
        message(1, 0, "please add a retry to the client", Some(10)),
        message(2, 3, PEER_TEXT, Some(11)),
        message(3, 6, PEER_TEXT, None),
        message(4, 9, "looks good, ship it now", Some(12)),
    ];
    let turns = vec![turn(10, 1, 0, Some(3))];
    let out = resolve_turns_for_facts(&ctx(&stream, 10, 12), &turns);
    let now = &out[0].now;
    assert_eq!(now.reply_window_segment_ids, vec![11]);
    assert_eq!(now.skipped.get("peer-message"), Some(&2));
    assert_eq!(now.next_genuine_request_id, Some(4));
}

#[test]
fn reclassification_flagged_when_third_repeat_turns_first_into_loop() -> Result<()> {
    let mut stream = vec![
        message(
            1,
            0,
            LOOP_TEXT
                .replace("Autonomous loop check", "Custom poll ping")
                .as_str(),
            Some(10),
        ),
        message(
            2,
            5,
            "Custom poll ping number 8 for the coordinator seat, keep polling",
            Some(11),
        ),
    ];
    let turns = vec![turn(10, 1, 0, Some(1))];
    let before = resolve_turns_for_facts(&ctx(&stream, 6, 7), &turns);
    assert_eq!(before[0].now.kind, "genuine");
    let prior = Prior {
        kind: before[0].now.kind.to_string(),
        rule: before[0].now.rule.map(str::to_string),
        next_genuine_request_id: before[0].now.next_genuine_request_id,
    };
    stream.push(message(
        3,
        20,
        "Custom poll ping number 9 for the coordinator seat, keep polling",
        Some(12),
    ));
    let after = resolve_turns_for_facts(&ctx(&stream, 21, 22), &turns);
    let now = &after[0].now;
    assert_eq!(now.kind, "loop-wakeup");
    assert_eq!(now.rule, Some("b"));
    assert_eq!(after[0].at_close.kind, "genuine");
    assert!(is_reclassification(&prior, now));
    Ok(())
}

#[test]
fn pending_to_resolved_is_not_reclassification() {
    let prior = Prior {
        kind: "genuine".to_string(),
        rule: None,
        next_genuine_request_id: None,
    };
    let stream = vec![
        message(1, 0, "please add a retry to the client", Some(10)),
        message(2, 5, "thanks, now add the docs", Some(11)),
    ];
    let out = resolve_turns_for_facts(&ctx(&stream, 6, 7), &[turn(10, 1, 0, Some(5))]);
    assert!(!is_reclassification(&prior, &out[0].now));
}

#[test]
fn next_genuine_change_between_values_is_reclassification() {
    let prior = Prior {
        kind: "genuine".to_string(),
        rule: None,
        next_genuine_request_id: Some(2),
    };
    let stream = vec![
        message(1, 0, "please add a retry to the client", Some(10)),
        message(3, 9, "a different follow up message here", Some(12)),
    ];
    let out = resolve_turns_for_facts(&ctx(&stream, 10, 12), &[turn(10, 1, 0, Some(5))]);
    assert!(is_reclassification(&prior, &out[0].now));
}

#[test]
fn opener_missing_from_stream_uses_created_position_and_opened_by() {
    let stream = vec![
        message(1, 0, "please add a retry to the client", Some(10)),
        message(9, 20, "next genuine request after the agent row", Some(13)),
    ];
    let mut agent_row = turn(11, 5, 4, Some(6));
    agent_row.opened_by = Some("agent".to_string());
    let machine_row = turn(12, 6, 5, Some(7));
    let out = resolve_turns_for_facts(&ctx(&stream, 21, 22), &[agent_row, machine_row]);
    assert_eq!(out[0].now.kind, "loop-wakeup");
    assert_eq!(out[0].now.rule, Some("c"));
    assert_eq!(out[0].now.next_genuine_request_id, Some(9));
    assert_eq!(out[1].now.kind, "empty");
    assert_eq!(out[1].now.rule, None);
}

#[test]
fn open_turns_are_not_resolved() {
    let stream = vec![message(1, 0, "please add a retry to the client", Some(10))];
    let out = resolve_turns_for_facts(&ctx(&stream, 1, 2), &[turn(10, 1, 0, None)]);
    assert!(out.is_empty());
}

#[test]
fn hindsight_matches_the_stream_resolver() -> Result<()> {
    let stream = vec![
        message(1, 0, "please add a retry to the client", Some(10)),
        message(2, 3, PEER_TEXT, Some(11)),
        message(3, 9, "looks good, ship it now", Some(12)),
        message(4, 20, "one more thing to change please", Some(13)),
    ];
    let turns: Vec<TurnRow> = vec![
        turn(10, 1, 0, Some(3)),
        turn(11, 2, 3, Some(9)),
        turn(12, 3, 9, Some(20)),
        turn(13, 4, 20, Some(25)),
    ];
    let mine = resolve_turns_for_facts(&ctx(&stream, 26, 27), &turns);
    for reference in resolve_turns(&stream, t(27)) {
        let ours = mine
            .iter()
            .find(|r| r.turn_id == reference.turn_id)
            .context("turn resolved")?;
        assert_eq!(
            ours.now.next_genuine_request_id,
            reference.next_genuine.as_ref().map(|n| n.request_id)
        );
        assert_eq!(ours.now.kind, reference.effective.kind.j2_code());
        assert_eq!(ours.now.skipped, reference.skipped);
        assert_eq!(
            ours.now.reply_window_segment_ids,
            reference.machine_segment_ids
        );
    }
    Ok(())
}
