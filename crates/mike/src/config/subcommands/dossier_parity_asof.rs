use std::collections::BTreeMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;

use super::DossierParityArgs;
use super::dossier_parity::{
    SessionData, horizon, load_sessions, population_of, write_json, write_jsonl,
};
use super::dossier_parity_compare::{J2Row, nonzero};
use crate::store::persistence::PersistHandle;
use crate::wire::genuine::stream::{
    Effective, NextGenuine, StreamMessage, TurnResolution, effective_kinds, resolve_turns,
};
use crate::wire::genuine::{DispositionClass, EffectiveRule, PromptKind};

#[derive(Debug, Clone)]
pub(super) struct AsOfView {
    pub own_kind: &'static str,
    pub own_rule: Option<EffectiveRule>,
    pub opened_by: &'static str,
    pub window_has_b: bool,
    pub next: Option<NextGenuine>,
    pub skipped: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Shape {
    Same,
    FutureOnly,
    HindsightB,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct AsOfCompare {
    pub turn_id: i64,
    pub class_diffs: Vec<&'static str>,
    pub window_has_b: bool,
    pub shape: Shape,
    pub reason: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct AsOfReport {
    pub unit: &'static str,
    pub turns: usize,
    pub turns_with_classification_difference: usize,
    pub classification_differences_outside_b_windows: Vec<i64>,
    pub shapes: BTreeMap<&'static str, usize>,
    pub failing_turns: Vec<i64>,
}

fn opened_by(kind: PromptKind) -> &'static str {
    match kind.class() {
        DispositionClass::Operator => "operator",
        DispositionClass::InLaneMachine | DispositionClass::Probe => "machine",
        DispositionClass::LaneForeign => "agent",
    }
}

pub(super) fn as_of_view(
    resolution: &TurnResolution,
    messages: &[StreamMessage],
    effective: &[Option<Effective>],
) -> AsOfView {
    let last = resolution.next_genuine.as_ref().map(|next| next.request_id);
    let window_has_b = messages.iter().zip(effective).any(|(message, eff)| {
        message.request_id >= resolution.request_id
            && last.is_none_or(|end| message.request_id <= end)
            && eff.is_some_and(|e| matches!(e.rule, Some(EffectiveRule::B | EffectiveRule::AB)))
    });
    AsOfView {
        own_kind: resolution.effective.kind.j2_code(),
        own_rule: resolution.effective.rule,
        opened_by: opened_by(resolution.effective.kind),
        window_has_b,
        next: resolution.next_genuine.clone(),
        skipped: nonzero(&resolution.skipped),
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Hindsight {
    pub close_next_is_b_at_horizon: bool,
    pub gap_is_non_genuine_at_horizon: bool,
}

fn hindsight_shape(
    close: &AsOfView,
    cutoff: &AsOfView,
    hindsight: Hindsight,
) -> (Shape, Option<String>) {
    let window = close.window_has_b || cutoff.window_has_b;
    let failed: Vec<&str> = [
        (
            hindsight.close_next_is_b_at_horizon,
            "the as-of-close next genuine message is not effective rule b/ab at the horizon",
        ),
        (
            hindsight.gap_is_non_genuine_at_horizon,
            "the horizon next genuine message is not the first one after M that is genuine at the horizon, or a skipped message is genuine at the horizon",
        ),
        (window, "the turn window has no effective b/ab message"),
    ]
    .into_iter()
    .filter(|(holds, _)| !holds)
    .map(|(_, reason)| reason)
    .collect();
    if failed.is_empty() {
        (Shape::HindsightB, None)
    } else {
        (Shape::Fail, Some(failed.join("; ")))
    }
}

fn next_shape(
    closed_at: DateTime<Utc>,
    close: &AsOfView,
    cutoff: &AsOfView,
    after_close: &BTreeMap<String, u32>,
    hindsight: Hindsight,
) -> (Shape, Option<String>) {
    let same_next =
        close.next.as_ref().map(|n| n.request_id) == cutoff.next.as_ref().map(|n| n.request_id);
    if same_next && close.skipped == cutoff.skipped {
        return (Shape::Same, None);
    }
    if close.next.is_some() {
        return hindsight_shape(close, cutoff, hindsight);
    }
    if cutoff
        .next
        .as_ref()
        .is_some_and(|next| next.at <= closed_at)
    {
        return (
            Shape::Fail,
            Some("as-of-cutoff next genuine message is dated at or before close".into()),
        );
    }
    let mut expected = close.skipped.clone();
    for (kind, count) in after_close {
        *expected.entry(kind.clone()).or_default() += count;
    }
    if expected == cutoff.skipped {
        (Shape::FutureOnly, None)
    } else {
        (
            Shape::Fail,
            Some("skipped counts differ by more than the messages created after close".into()),
        )
    }
}

pub(super) fn compare_as_of(
    turn: &J2Row,
    close: &AsOfView,
    cutoff: &AsOfView,
    after_close: &BTreeMap<String, u32>,
    hindsight: Hindsight,
) -> AsOfCompare {
    let mut class_diffs = Vec::new();
    if close.own_kind != cutoff.own_kind {
        class_diffs.push("own_kind");
    }
    if close.own_rule != cutoff.own_rule {
        class_diffs.push("own_rule");
    }
    if close.opened_by != cutoff.opened_by {
        class_diffs.push("opened_by");
    }
    let (shape, reason) = next_shape(turn.closed_at, close, cutoff, after_close, hindsight);
    AsOfCompare {
        turn_id: turn.turn_id,
        class_diffs,
        window_has_b: close.window_has_b || cutoff.window_has_b,
        shape,
        reason,
    }
}

pub(super) fn summarise_as_of(compares: &[AsOfCompare]) -> AsOfReport {
    let mut report = AsOfReport {
        unit: "turns",
        turns: compares.len(),
        ..AsOfReport::default()
    };
    for compare in compares {
        let shape = match compare.shape {
            Shape::Same => "same",
            Shape::FutureOnly => "future_only",
            Shape::HindsightB => "hindsight_b",
            Shape::Fail => "fail",
        };
        *report.shapes.entry(shape).or_default() += 1;
        if compare.shape == Shape::Fail {
            report.failing_turns.push(compare.turn_id);
        }
        if !compare.class_diffs.is_empty() {
            report.turns_with_classification_difference += 1;
            if !compare.window_has_b {
                report
                    .classification_differences_outside_b_windows
                    .push(compare.turn_id);
            }
        }
    }
    report
}

fn hindsight_facts(
    session: &SessionData,
    horizon_eff: &[Option<Effective>],
    close: &TurnResolution,
    cutoff: &TurnResolution,
) -> Hindsight {
    let kind_at = |request_id: i64| {
        session
            .messages
            .iter()
            .position(|m| m.request_id == request_id)
            .and_then(|index| horizon_eff.get(index).copied().flatten())
    };
    let close_next_is_b_at_horizon = close
        .next_genuine
        .as_ref()
        .and_then(|next| kind_at(next.request_id))
        .is_some_and(|eff| {
            eff.kind != PromptKind::Genuine
                && matches!(eff.rule, Some(EffectiveRule::B | EffectiveRule::AB))
        });
    let gap_is_non_genuine_at_horizon = close.next_genuine.as_ref().is_some_and(|m| {
        let end = cutoff.next_genuine.as_ref().map(|next| next.request_id);
        end.is_none_or(|end| end > m.request_id)
            && session
                .messages
                .iter()
                .zip(horizon_eff)
                .filter(|(message, _)| {
                    message.request_id > m.request_id
                        && end.is_none_or(|end| message.request_id < end)
                })
                .all(|(_, eff)| eff.is_some_and(|e| e.kind != PromptKind::Genuine))
    });
    Hindsight {
        close_next_is_b_at_horizon,
        gap_is_non_genuine_at_horizon,
    }
}

pub(super) fn after_close_counts(
    session: &SessionData,
    effective: &[Option<Effective>],
    turn: &J2Row,
    cutoff: &TurnResolution,
) -> BTreeMap<String, u32> {
    let end = cutoff.next_genuine.as_ref().map(|next| next.request_id);
    let mut counts = BTreeMap::new();
    for (message, eff) in session.messages.iter().zip(effective) {
        let Some(eff) = eff else { continue };
        if message.request_id > cutoff.request_id
            && message.at > turn.closed_at
            && end.is_none_or(|end| message.request_id < end)
        {
            *counts.entry(eff.kind.j2_code().to_string()).or_default() += 1;
        }
    }
    counts
}

fn compare_session(session: &SessionData, args: &DossierParityArgs) -> Vec<AsOfCompare> {
    let horizon_at = horizon(args);
    let at_horizon = resolve_turns(&session.messages, horizon_at);
    let horizon_eff = effective_kinds(&session.messages, horizon_at);
    let mut compares = Vec::new();
    for turn in &session.turns {
        let close_res = resolve_turns(&session.messages, turn.closed_at);
        let close_eff = effective_kinds(&session.messages, turn.closed_at);
        let (Some(close), Some(cutoff)) = (
            close_res.iter().find(|r| r.turn_id == turn.turn_id),
            at_horizon.iter().find(|r| r.turn_id == turn.turn_id),
        ) else {
            continue;
        };
        let after = after_close_counts(session, &horizon_eff, turn, cutoff);
        let facts = hindsight_facts(session, &horizon_eff, close, cutoff);
        compares.push(compare_as_of(
            turn,
            &as_of_view(close, &session.messages, &close_eff),
            &as_of_view(cutoff, &session.messages, &horizon_eff),
            &after,
            facts,
        ));
    }
    compares
}

pub(super) async fn run_as_of_comparison(
    ph: &PersistHandle,
    args: &DossierParityArgs,
) -> Result<()> {
    let population = population_of(args)?;
    let sessions = load_sessions(ph, args, &population).await?;
    let compares: Vec<AsOfCompare> = sessions
        .iter()
        .flat_map(|session| compare_session(session, args))
        .collect();
    let report = summarise_as_of(&compares);
    let listed: Vec<&AsOfCompare> = compares
        .iter()
        .filter(|c| !c.class_diffs.is_empty() || matches!(c.shape, Shape::Fail | Shape::HindsightB))
        .collect();
    write_json(&args.out.join("as-of-comparison.json"), &report)?;
    write_jsonl(&args.out.join("as-of-comparison-turns.jsonl"), &listed)?;
    println!(
        "dossier-parity --compare-as-of: {} turns, {} classification differences ({} outside a b/ab window), shapes {:?}",
        report.turns,
        report.turns_with_classification_difference,
        report.classification_differences_outside_b_windows.len(),
        report.shapes
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn next_shape_accepts_only_messages_created_after_close() {
        let closed_at = Utc
            .with_ymd_and_hms(2026, 9, 1, 0, 0, 0)
            .single()
            .unwrap_or_else(Utc::now);
        let view = |skipped: &[(&str, u32)]| AsOfView {
            own_kind: "genuine",
            own_rule: None,
            opened_by: "operator",
            window_has_b: false,
            next: None,
            skipped: skipped.iter().map(|(k, n)| (k.to_string(), *n)).collect(),
        };
        let after = BTreeMap::from([("loop-wakeup".to_string(), 2)]);
        let (same, _) = next_shape(
            closed_at,
            &view(&[]),
            &view(&[]),
            &BTreeMap::new(),
            Hindsight::default(),
        );
        assert_eq!(same, Shape::Same);
        let (future, _) = next_shape(
            closed_at,
            &view(&[]),
            &view(&[("loop-wakeup", 2)]),
            &after,
            Hindsight::default(),
        );
        assert_eq!(future, Shape::FutureOnly);
        let (fail, _) = next_shape(
            closed_at,
            &view(&[]),
            &view(&[("loop-wakeup", 3)]),
            &after,
            Hindsight::default(),
        );
        assert_eq!(fail, Shape::Fail);
    }

    #[test]
    fn hindsight_shape_requires_all_three_conditions() {
        let with_next = |request_id: i64, has_b: bool| AsOfView {
            own_kind: "genuine",
            own_rule: None,
            opened_by: "operator",
            window_has_b: has_b,
            next: Utc
                .with_ymd_and_hms(2026, 9, 1, 0, 0, 0)
                .single()
                .map(|at| NextGenuine {
                    request_id,
                    turn_id: None,
                    at,
                }),
            skipped: BTreeMap::new(),
        };
        let all = Hindsight {
            close_next_is_b_at_horizon: true,
            gap_is_non_genuine_at_horizon: true,
        };
        assert_eq!(
            hindsight_shape(&with_next(5, true), &with_next(9, true), all).0,
            Shape::HindsightB
        );
        let no_b = Hindsight {
            close_next_is_b_at_horizon: false,
            ..all
        };
        assert_eq!(
            hindsight_shape(&with_next(5, true), &with_next(9, true), no_b).0,
            Shape::Fail
        );
        let no_gap = Hindsight {
            gap_is_non_genuine_at_horizon: false,
            ..all
        };
        assert_eq!(
            hindsight_shape(&with_next(5, true), &with_next(9, true), no_gap).0,
            Shape::Fail
        );
        assert_eq!(
            hindsight_shape(&with_next(5, false), &with_next(9, false), all).0,
            Shape::Fail
        );
    }
}
