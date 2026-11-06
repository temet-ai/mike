use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};

use super::repeat::qualifying_keys;
use super::{CaptureRule, EffectiveRule, PromptClass, PromptKind};

#[derive(Debug, Clone)]
pub struct StreamMessage {
    pub request_id: i64,
    pub at: DateTime<Utc>,
    pub capture: PromptClass,
    pub repeat_key: Option<String>,
    pub turn_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effective {
    pub kind: PromptKind,
    pub rule: Option<EffectiveRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextGenuine {
    pub request_id: i64,
    pub turn_id: Option<i64>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnResolution {
    pub turn_id: i64,
    pub request_id: i64,
    pub effective: Effective,
    pub next_genuine: Option<NextGenuine>,
    pub skipped: BTreeMap<&'static str, u32>,
    pub machine_segment_ids: Vec<i64>,
}

fn capture_rule(rule: Option<CaptureRule>) -> Option<EffectiveRule> {
    rule.map(|rule| match rule {
        CaptureRule::A => EffectiveRule::A,
        CaptureRule::C => EffectiveRule::C,
        CaptureRule::Envelope => EffectiveRule::Envelope,
    })
}

fn effective_of(message: &StreamMessage, qualifying: &HashSet<&str>) -> Effective {
    let in_group = message
        .repeat_key
        .as_deref()
        .is_some_and(|key| qualifying.contains(key));
    let capture = message.capture;
    match (in_group, capture.kind, capture.rule) {
        (true, PromptKind::Genuine, _) => Effective {
            kind: PromptKind::LoopWakeup,
            rule: Some(EffectiveRule::B),
        },
        (true, PromptKind::LoopWakeup, Some(CaptureRule::A)) => Effective {
            kind: PromptKind::LoopWakeup,
            rule: Some(EffectiveRule::AB),
        },
        _ => Effective {
            kind: capture.kind,
            rule: capture_rule(capture.rule),
        },
    }
}

pub fn effective_kinds(stream: &[StreamMessage], as_of: DateTime<Utc>) -> Vec<Option<Effective>> {
    let entries = stream
        .iter()
        .filter(|message| message.at <= as_of)
        .filter_map(|message| message.repeat_key.as_deref().map(|key| (key, message.at)));
    let qualifying = qualifying_keys(entries);
    stream
        .iter()
        .map(|message| (message.at <= as_of).then(|| effective_of(message, &qualifying)))
        .collect()
}

fn resolve_one(
    stream: &[StreamMessage],
    effective: &[Option<Effective>],
    index: usize,
    turn_id: i64,
) -> Option<TurnResolution> {
    let own = effective[index]?;
    let mut skipped: BTreeMap<&'static str, u32> = BTreeMap::new();
    let mut machine_segment_ids = Vec::new();
    let mut next_genuine = None;
    for (offset, later) in effective.iter().enumerate().skip(index + 1) {
        let Some(later) = later else { continue };
        let message = &stream[offset];
        if later.kind == PromptKind::Genuine {
            next_genuine = Some(NextGenuine {
                request_id: message.request_id,
                turn_id: message.turn_id,
                at: message.at,
            });
            break;
        }
        *skipped.entry(later.kind.j2_code()).or_insert(0) += 1;
        machine_segment_ids.extend(message.turn_id);
    }
    Some(TurnResolution {
        turn_id,
        request_id: stream[index].request_id,
        effective: own,
        next_genuine,
        skipped,
        machine_segment_ids,
    })
}

pub fn resolve_turns(stream: &[StreamMessage], as_of: DateTime<Utc>) -> Vec<TurnResolution> {
    let effective = effective_kinds(stream, as_of);
    stream
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            message
                .turn_id
                .and_then(|turn_id| resolve_one(stream, &effective, index, turn_id))
        })
        .collect()
}
