use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::wire::genuine::disposition::TurnDisposition;
use crate::wire::genuine::stream::TurnResolution;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct J2Row {
    pub turn_id: i64,
    pub closed_at: DateTime<Utc>,
    pub session_uuid: String,
    pub own_prompt_kind: String,
    pub next_genuine_turn_id: Option<i64>,
    pub next_genuine_request_id: Option<i64>,
    #[serde(default)]
    pub skipped_before_next_genuine: Option<BTreeMap<String, u32>>,
    #[serde(default)]
    pub session_ends_without_next_genuine: bool,
    #[serde(default)]
    pub in_sample: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MikeRow {
    pub own_kind: String,
    pub next_turn_id: Option<i64>,
    pub next_request_id: Option<i64>,
    pub skipped: BTreeMap<String, u32>,
    pub ends_without_next: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct FieldDiff {
    pub turn_id: i64,
    pub field: &'static str,
    pub j2: String,
    pub mike: String,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct ParitySummary {
    pub population: usize,
    pub compared: usize,
    pub unmapped_turns: Vec<i64>,
    pub j2_kind_counts: BTreeMap<String, usize>,
    pub mike_kind_counts: BTreeMap<String, usize>,
    pub j2_without_next_genuine: usize,
    pub mike_without_next_genuine: usize,
    pub differing_turns: usize,
    pub differences_by_field: BTreeMap<&'static str, usize>,
    pub sample_turns: usize,
    pub sample_differing_turns: usize,
    pub control_turns: usize,
    pub control_differing_turns: usize,
    pub horizon: Option<DateTime<Utc>>,
    pub as_of: String,
    pub source: String,
}

pub(super) fn mike_row(resolution: &TurnResolution) -> MikeRow {
    MikeRow {
        own_kind: resolution.effective.kind.j2_code().to_string(),
        next_turn_id: resolution
            .next_genuine
            .as_ref()
            .and_then(|next| next.turn_id),
        next_request_id: resolution.next_genuine.as_ref().map(|next| next.request_id),
        skipped: nonzero(&resolution.skipped),
        ends_without_next: resolution.next_genuine.is_none(),
    }
}

pub(super) fn nonzero<'a, K: ToString + 'a>(
    counts: impl IntoIterator<Item = (K, &'a u32)>,
) -> BTreeMap<String, u32> {
    counts
        .into_iter()
        .filter(|(_, count)| **count > 0)
        .map(|(kind, count)| (kind.to_string(), *count))
        .collect()
}

fn opt(value: Option<i64>) -> String {
    value.map_or_else(|| "null".into(), |v| v.to_string())
}

pub(super) fn diff_row(j2: &J2Row, mike: &MikeRow) -> Vec<FieldDiff> {
    let mut diffs = Vec::new();
    let mut push = |field: &'static str, left: String, right: String| {
        if left != right {
            diffs.push(FieldDiff {
                turn_id: j2.turn_id,
                field,
                j2: left,
                mike: right,
            });
        }
    };
    push(
        "own_prompt_kind",
        j2.own_prompt_kind.clone(),
        mike.own_kind.clone(),
    );
    push(
        "next_genuine_turn_id",
        opt(j2.next_genuine_turn_id),
        opt(mike.next_turn_id),
    );
    push(
        "next_genuine_request_id",
        opt(j2.next_genuine_request_id),
        opt(mike.next_request_id),
    );
    push(
        "skipped_before_next_genuine",
        format!(
            "{:?}",
            nonzero(j2.skipped_before_next_genuine.iter().flatten())
        ),
        format!("{:?}", mike.skipped),
    );
    push(
        "session_ends_without_next_genuine",
        j2.session_ends_without_next_genuine.to_string(),
        mike.ends_without_next.to_string(),
    );
    diffs
}

pub(super) fn summarise(
    population: &[J2Row],
    mikes: &BTreeMap<i64, MikeRow>,
    diffs: &[FieldDiff],
) -> ParitySummary {
    let mut summary = ParitySummary {
        population: population.len(),
        ..ParitySummary::default()
    };
    for row in population {
        *summary
            .j2_kind_counts
            .entry(row.own_prompt_kind.clone())
            .or_default() += 1;
        if row.session_ends_without_next_genuine {
            summary.j2_without_next_genuine += 1;
        }
        match mikes.get(&row.turn_id) {
            Some(mike) => {
                summary.compared += 1;
                *summary
                    .mike_kind_counts
                    .entry(mike.own_kind.clone())
                    .or_default() += 1;
                if mike.ends_without_next {
                    summary.mike_without_next_genuine += 1;
                }
            }
            None => summary.unmapped_turns.push(row.turn_id),
        }
    }
    let mut turns: Vec<i64> = diffs.iter().map(|d| d.turn_id).collect();
    turns.dedup();
    summary.differing_turns = turns.len();
    let differs = |id: i64| turns.contains(&id);
    for row in population {
        if row.in_sample && mikes.contains_key(&row.turn_id) {
            summary.sample_turns += 1;
            summary.sample_differing_turns += usize::from(differs(row.turn_id));
        }
        if CONTROL_TURN_IDS.contains(&row.turn_id) && mikes.contains_key(&row.turn_id) {
            summary.control_turns += 1;
            summary.control_differing_turns += usize::from(differs(row.turn_id));
        }
    }
    for diff in diffs {
        *summary.differences_by_field.entry(diff.field).or_default() += 1;
    }
    summary
}

pub(super) const CONTROL_TURN_IDS: [i64; 13] = [
    16368, 14959, 15395, 15398, 15399, 15400, 15401, 15402, 15403, 15404, 15405, 14753, 15614,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CaptureEffect {
    pub opened: bool,
    pub closed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DispositionChange {
    LosesOperatorTurn,
    OperatorTurnBecomesMachineSegment,
    NewMachineSegment,
    CloseAvoided,
    Unchanged,
}

pub(super) fn disposition_change(effect: CaptureEffect, now: TurnDisposition) -> DispositionChange {
    let now_opens = matches!(
        now,
        TurnDisposition::OperatorTurn | TurnDisposition::MachineSegment
    );
    match (effect.opened, now) {
        (true, TurnDisposition::Attach | TurnDisposition::Ignore) => {
            DispositionChange::LosesOperatorTurn
        }
        (true, TurnDisposition::MachineSegment) => {
            DispositionChange::OperatorTurnBecomesMachineSegment
        }
        (false, TurnDisposition::MachineSegment) => DispositionChange::NewMachineSegment,
        _ if effect.closed && !now_opens => DispositionChange::CloseAvoided,
        _ => DispositionChange::Unchanged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::genuine::PromptKind;
    use crate::wire::genuine::stream::{Effective, NextGenuine};
    use chrono::TimeZone;

    fn resolution(next: Option<i64>, skipped: &[(&'static str, u32)]) -> TurnResolution {
        let at = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).single();
        TurnResolution {
            turn_id: 1,
            request_id: 10,
            effective: Effective {
                kind: PromptKind::Genuine,
                rule: None,
            },
            next_genuine: at.zip(next).map(|(at, id)| NextGenuine {
                request_id: id * 10,
                turn_id: Some(id),
                at,
            }),
            skipped: skipped.iter().copied().collect(),
            machine_segment_ids: Vec::new(),
        }
    }

    fn j2(next: Option<i64>, skipped: Option<BTreeMap<String, u32>>) -> J2Row {
        J2Row {
            turn_id: 1,
            closed_at: Utc::now(),
            session_uuid: "s".to_string(),
            own_prompt_kind: "genuine".to_string(),
            next_genuine_turn_id: next,
            next_genuine_request_id: next.map(|id| id * 10),
            skipped_before_next_genuine: skipped,
            session_ends_without_next_genuine: next.is_none(),
            in_sample: false,
        }
    }

    #[test]
    fn summarise_counts_unmapped_turns_and_differences() {
        let population = vec![j2(Some(2), None), {
            let mut other = j2(None, None);
            other.turn_id = 9;
            other
        }];
        let mikes = BTreeMap::from([(1, mike_row(&resolution(Some(3), &[])))]);
        let diffs = diff_row(&population[0], &mikes[&1]);
        let summary = summarise(&population, &mikes, &diffs);
        assert_eq!(summary.unmapped_turns, vec![9]);
        assert_eq!(summary.differing_turns, 1);
    }

    #[test]
    fn disposition_change_covers_each_class() {
        let effect = |opened, closed| CaptureEffect { opened, closed };
        let cases = [
            (
                effect(true, true),
                TurnDisposition::Attach,
                DispositionChange::LosesOperatorTurn,
            ),
            (
                effect(true, true),
                TurnDisposition::MachineSegment,
                DispositionChange::OperatorTurnBecomesMachineSegment,
            ),
            (
                effect(false, true),
                TurnDisposition::MachineSegment,
                DispositionChange::NewMachineSegment,
            ),
            (
                effect(false, true),
                TurnDisposition::Ignore,
                DispositionChange::CloseAvoided,
            ),
            (
                effect(true, true),
                TurnDisposition::OperatorTurn,
                DispositionChange::Unchanged,
            ),
            (
                effect(false, false),
                TurnDisposition::Attach,
                DispositionChange::Unchanged,
            ),
        ];
        for (capture, now, expected) in cases {
            assert_eq!(disposition_change(capture, now), expected);
        }
    }
}
