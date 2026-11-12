use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Duration, Utc};

use crate::wire::genuine::stream::{Effective, StreamMessage, effective_kinds};
use crate::wire::genuine::{EffectiveRule, PromptKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextState {
    Resolved,
    Pending,
    NoneSessionSettled,
}

impl NextState {
    pub fn code(self) -> &'static str {
        match self {
            NextState::Resolved => "resolved",
            NextState::Pending => "pending",
            NextState::NoneSessionSettled => "none_session_settled",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TurnRow {
    pub turn_id: i64,
    pub start_request_id: i64,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub opened_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsOf {
    pub kind: &'static str,
    pub rule: Option<&'static str>,
    pub next_state: NextState,
    pub next_genuine_request_id: Option<i64>,
    pub next_genuine_turn_id: Option<i64>,
    pub next_genuine_at: Option<DateTime<Utc>>,
    pub skipped: BTreeMap<&'static str, u32>,
    pub reply_window_segment_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnResolved {
    pub turn_id: i64,
    pub now: AsOf,
    pub at_close: AsOf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prior {
    pub kind: String,
    pub rule: Option<String>,
    pub next_genuine_request_id: Option<i64>,
}

pub struct ResolveContext<'a> {
    pub stream: &'a [StreamMessage],
    pub last_seen: DateTime<Utc>,
    pub now: DateTime<Utc>,
    pub settle: Duration,
}

pub fn rule_code(rule: EffectiveRule) -> &'static str {
    match rule {
        EffectiveRule::A => "a",
        EffectiveRule::B => "b",
        EffectiveRule::AB => "ab",
        EffectiveRule::C => "c",
        EffectiveRule::Envelope => "envelope",
    }
}

fn own_kind(
    stream: &[StreamMessage],
    effective: &[Option<Effective>],
    turn: &TurnRow,
) -> (&'static str, Option<&'static str>) {
    let found = stream
        .binary_search_by_key(&turn.start_request_id, |message| message.request_id)
        .ok()
        .and_then(|index| effective[index]);
    match (found, turn.opened_by.as_deref()) {
        (Some(own), _) => (own.kind.j2_code(), own.rule.map(rule_code)),
        (None, Some("agent")) => (
            PromptKind::AgentLineage.j2_code(),
            Some(rule_code(EffectiveRule::C)),
        ),
        (None, _) => (PromptKind::Empty.j2_code(), None),
    }
}

struct Following {
    next: Option<usize>,
    skipped: BTreeMap<&'static str, u32>,
    segments: Vec<i64>,
}

fn scan_following(
    stream: &[StreamMessage],
    effective: &[Option<Effective>],
    after_request_id: i64,
) -> Following {
    let first = stream.partition_point(|message| message.request_id <= after_request_id);
    let mut following = Following {
        next: None,
        skipped: BTreeMap::new(),
        segments: Vec::new(),
    };
    for (offset, later) in effective.iter().enumerate().skip(first) {
        let Some(later) = later else { continue };
        if later.kind == PromptKind::Genuine {
            following.next = Some(offset);
            break;
        }
        *following.skipped.entry(later.kind.j2_code()).or_insert(0) += 1;
        following.segments.extend(stream[offset].turn_id);
    }
    following
}

fn next_state(
    resolved: bool,
    last_seen: DateTime<Utc>,
    as_of: DateTime<Utc>,
    settle: Duration,
) -> NextState {
    if resolved {
        NextState::Resolved
    } else if last_seen < as_of - settle {
        NextState::NoneSessionSettled
    } else {
        NextState::Pending
    }
}

fn resolve_with(
    ctx: &ResolveContext<'_>,
    effective: &[Option<Effective>],
    turn: &TurnRow,
    as_of: DateTime<Utc>,
) -> AsOf {
    let (kind, rule) = own_kind(ctx.stream, effective, turn);
    let following = scan_following(ctx.stream, effective, turn.start_request_id);
    let next = following.next.map(|index| &ctx.stream[index]);
    AsOf {
        kind,
        rule,
        next_state: next_state(next.is_some(), ctx.last_seen, as_of, ctx.settle),
        next_genuine_request_id: next.map(|message| message.request_id),
        next_genuine_turn_id: next.and_then(|message| message.turn_id),
        next_genuine_at: next.map(|message| message.at),
        skipped: following.skipped,
        reply_window_segment_ids: following.segments,
    }
}

pub fn resolve_selective(
    ctx: &ResolveContext<'_>,
    turns: &[TurnRow],
    at_close_for: &HashSet<i64>,
) -> Vec<TurnResolved> {
    let hindsight = effective_kinds(ctx.stream, ctx.now);
    turns
        .iter()
        .filter_map(|turn| {
            let closed_at = turn.closed_at?;
            let now = resolve_with(ctx, &hindsight, turn, ctx.now);
            let at_close = if at_close_for.contains(&turn.turn_id) {
                let effective = effective_kinds(ctx.stream, closed_at);
                resolve_with(ctx, &effective, turn, closed_at)
            } else {
                now.clone()
            };
            Some(TurnResolved {
                turn_id: turn.turn_id,
                now,
                at_close,
            })
        })
        .collect()
}

pub fn resolve_turns_for_facts(ctx: &ResolveContext<'_>, turns: &[TurnRow]) -> Vec<TurnResolved> {
    let all: HashSet<i64> = turns.iter().map(|turn| turn.turn_id).collect();
    resolve_selective(ctx, turns, &all)
}

pub fn is_reclassification(prior: &Prior, current: &AsOf) -> bool {
    let kind_changed = prior.kind != current.kind;
    let rule_changed = prior.rule.as_deref() != current.rule;
    let next_changed = match (
        prior.next_genuine_request_id,
        current.next_genuine_request_id,
    ) {
        (Some(before), after) => Some(before) != after,
        (None, _) => false,
    };
    kind_changed || rule_changed || next_changed
}
