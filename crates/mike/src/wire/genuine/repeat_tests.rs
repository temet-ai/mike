use chrono::{DateTime, Duration, TimeZone, Utc};

use super::repeat::{qualifying_keys, repeat_key};
use super::stream::{StreamMessage, effective_kinds};
use super::{CaptureRule, EffectiveRule, PromptClass, PromptKind};

const LONG: &str = "please look at the failing build again and report back to me";

fn epoch() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0)
        .single()
        .unwrap_or_default()
}

fn message(
    id: i64,
    seconds: i64,
    kind: PromptKind,
    rule: Option<CaptureRule>,
    text: &str,
) -> StreamMessage {
    StreamMessage {
        request_id: id,
        at: epoch() + Duration::seconds(seconds),
        capture: PromptClass { kind, rule },
        repeat_key: repeat_key(text),
        turn_id: None,
    }
}

fn genuine(id: i64, seconds: i64) -> StreamMessage {
    message(id, seconds, PromptKind::Genuine, None, LONG)
}

fn far_future() -> DateTime<Utc> {
    epoch() + Duration::days(30)
}

#[test]
fn repeat_key_under_40_chars_is_none() {
    assert_eq!(repeat_key("too short"), None);
    assert!(repeat_key(LONG).is_some());
}

#[test]
fn repeat_key_strips_uuids_then_digits_then_collapses_whitespace() {
    let text =
        "  Check   Session 123e4567-e89b-12d3-a456-426614174000 attempt 42\tnow please, thanks  ";
    assert_eq!(
        repeat_key(text).as_deref(),
        Some("check session attempt now please, thanks")
    );
}

#[test]
fn repeat_key_counts_120_chars_not_bytes() {
    let text = "é".repeat(200);
    let key = repeat_key(&text).unwrap_or_default();
    assert_eq!(key.chars().count(), 120);
    assert_eq!(key.len(), 240);
}

#[test]
fn rule_b_burst_with_median_gap_under_120s_stays_genuine() {
    let stream = vec![
        genuine(1, 0),
        genuine(2, 10),
        genuine(3, 20),
        genuine(4, 30),
    ];
    let kinds = effective_kinds(&stream, far_future());
    assert!(
        kinds
            .iter()
            .flatten()
            .all(|e| e.kind == PromptKind::Genuine)
    );
}

#[test]
fn rule_b_three_repeats_with_median_gap_at_least_120s_is_loop_wakeup() {
    let stream = vec![genuine(1, 0), genuine(2, 200), genuine(3, 400)];
    let kinds = effective_kinds(&stream, far_future());
    for effective in kinds.into_iter().flatten() {
        assert_eq!(effective.kind, PromptKind::LoopWakeup);
        assert_eq!(effective.rule, Some(EffectiveRule::B));
    }
}

fn keys_of(stream: &[StreamMessage]) -> usize {
    qualifying_keys(
        stream
            .iter()
            .filter_map(|m| m.repeat_key.as_deref().map(|k| (k, m.at))),
    )
    .len()
}

#[test]
fn rule_b_even_gap_count_uses_mean_of_middle_gaps() {
    let qualifies = vec![
        genuine(1, 0),
        genuine(2, 10),
        genuine(3, 260),
        genuine(4, 520),
        genuine(5, 540),
    ];
    assert_eq!(keys_of(&qualifies), 1);
    let below = vec![
        genuine(1, 0),
        genuine(2, 100),
        genuine(3, 200),
        genuine(4, 330),
        genuine(5, 630),
    ];
    assert_eq!(keys_of(&below), 0);
}

#[test]
fn rule_b_as_of_before_third_occurrence_stays_genuine() {
    let stream = vec![genuine(1, 0), genuine(2, 200), genuine(3, 400)];
    let kinds = effective_kinds(&stream, epoch() + Duration::seconds(250));
    assert_eq!(kinds[0].map(|e| e.kind), Some(PromptKind::Genuine));
    assert_eq!(kinds[1].map(|e| e.kind), Some(PromptKind::Genuine));
    assert_eq!(kinds[2], None);
}

#[test]
fn rule_b_template_loop_in_qualifying_group_is_ab() {
    let stream: Vec<StreamMessage> = (0..3)
        .map(|i| {
            message(
                i + 1,
                i * 300,
                PromptKind::LoopWakeup,
                Some(CaptureRule::A),
                LONG,
            )
        })
        .collect();
    let kinds = effective_kinds(&stream, far_future());
    for effective in kinds.into_iter().flatten() {
        assert_eq!(effective.rule, Some(EffectiveRule::AB));
    }
}

#[test]
fn rule_c_wins_over_b() {
    let stream: Vec<StreamMessage> = (0..3)
        .map(|i| {
            message(
                i + 1,
                i * 300,
                PromptKind::AgentLineage,
                Some(CaptureRule::C),
                LONG,
            )
        })
        .collect();
    let kinds = effective_kinds(&stream, far_future());
    for effective in kinds.into_iter().flatten() {
        assert_eq!(effective.kind, PromptKind::AgentLineage);
        assert_eq!(effective.rule, Some(EffectiveRule::C));
    }
}

#[test]
fn rule_b_negative_gap_from_out_of_order_times_is_counted_signed() {
    let stream = vec![genuine(1, 1000), genuine(2, 0), genuine(3, 100)];
    assert_eq!(keys_of(&stream), 0);
}
