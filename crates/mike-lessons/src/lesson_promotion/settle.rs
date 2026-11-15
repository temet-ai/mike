use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;

use super::budget::{CapInputs, enforce_cap};
use super::ledger::{
    Emitted, Ledger, Measured, record, retire_corrected_above_base, retire_unloaded,
};
use super::plan::SkillPlan;

pub(super) struct SettleInputs<'a> {
    pub today: NaiveDate,
    pub loads: &'a BTreeMap<String, i64>,
    pub cap: usize,
    pub measured: &'a Measured,
}

#[derive(Debug, Default)]
pub(super) struct Settled {
    pub retired: Vec<String>,
    pub harmful: Vec<String>,
    pub capped: Vec<String>,
    pub held_for_cap: Vec<String>,
    pub authored: usize,
}

fn emitted_paths(plan: &SkillPlan) -> Emitted {
    plan.emitted
        .iter()
        .map(|(key, emission)| (key.clone(), emission.path.clone()))
        .collect()
}

fn withdraw(plan: &mut SkillPlan, key: &str) {
    if let Some(emission) = plan.emitted.remove(key) {
        plan.desired.remove(&emission.path.display().to_string());
    }
    for extra in plan.extra.remove(key).unwrap_or_default() {
        plan.desired.remove(&extra);
    }
}

pub(super) fn settle(
    plan: &mut SkillPlan,
    ledger: &mut Ledger,
    inputs: &SettleInputs<'_>,
) -> Settled {
    let today = inputs.today;
    let all = emitted_paths(plan);
    let retired = retire_unloaded(&mut plan.desired, ledger, inputs.loads, today, &all);
    let remaining: Emitted = all
        .iter()
        .filter(|(key, _)| !retired.contains(key))
        .map(|(key, path)| (key.clone(), path.clone()))
        .collect();
    let harmful = retire_corrected_above_base(
        &mut plan.desired,
        ledger,
        inputs.measured,
        today,
        &remaining,
    );
    record(ledger, inputs.measured);
    for key in retired.iter().chain(&harmful) {
        withdraw(plan, key);
    }
    let live: BTreeSet<String> = plan.emitted.keys().cloned().collect();
    let reviewed: BTreeSet<String> = plan.authored.intersection(&live).cloned().collect();
    let lessons: BTreeMap<String, usize> = live
        .iter()
        .map(|key| {
            (
                key.clone(),
                plan.candidate_lessons.get(key).copied().unwrap_or(0),
            )
        })
        .collect();
    let (capped, held_for_cap) = enforce_cap(
        &CapInputs {
            ledger,
            loads: inputs.loads,
            today,
            live: &live,
            fresh: &plan.fresh,
            lessons: &lessons,
            reviewed: &reviewed,
        },
        inputs.cap,
    );
    for key in &capped {
        if let Some(entry) = ledger.get_mut(key) {
            entry.retired_at = Some(today.to_string());
        }
    }
    for key in capped.iter().chain(&held_for_cap) {
        withdraw(plan, key);
    }
    let authored = plan
        .authored
        .iter()
        .filter(|key| plan.emitted.contains_key(*key))
        .count();
    ledger.retain(|key, entry| plan.emitted.contains_key(key) || entry.retired_at.is_some());
    let mut retired = retired;
    retired.extend(capped.iter().cloned());
    Settled {
        retired,
        harmful,
        capped,
        held_for_cap,
        authored,
    }
}
