pub mod compose;
pub mod questions;
pub mod state;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::LazyLock;

use dashmap::DashMap;
use serde_json::json;

pub use compose::{DECIDE_AT, Decided, Decision, compose, rewrite};
pub use questions::{POINT, QUESTION_SET};
pub use state::Unit;

use crate::llm::decisions::env::{DecisionEnv, Pace};
use crate::llm::decisions::point::{Asked, Decider};
use crate::llm::decisions::record::DecisionSubject;
use crate::store::persistence::dossier_entries::{DirectiveSourceRow, NewDossierEntry};

const GROUP: usize = 6;
const MAX_CALLS: usize = 4;
const MAX_REMEMBERED: usize = 50_000;

static MEMO: LazyLock<DashMap<(i64, &'static str), Option<Decision>>> = LazyLock::new(DashMap::new);

fn remember(unit: &Unit, decided: Option<Decision>) {
    if MEMO.len() > MAX_REMEMBERED {
        MEMO.clear();
    }
    MEMO.insert((unit.request_id, unit.class), decided);
}

fn pending(units: &[Unit]) -> Vec<&Unit> {
    units
        .iter()
        .filter(|u| !MEMO.contains_key(&(u.request_id, u.class)))
        .take(GROUP * MAX_CALLS)
        .collect()
}

async fn ask_group(decider: &Decider, group: &[&Unit]) -> Asked {
    let asked = decider
        .ask(
            QUESTION_SET,
            state::state(group),
            questions::questions(group),
            DecisionSubject {
                session_id: group.first().map(|u| u.session_id),
                operator_turn_id: group.first().map(|u| u.turn_id),
                request_id: group.first().map(|u| u.request_id),
                ..DecisionSubject::default()
            },
            json!({ "decide_at": DECIDE_AT }),
        )
        .await;
    let found = compose(&asked, group);
    for d in &found {
        remember(group[d.index], d.decision);
    }
    let decided = found.iter().filter(|d| d.decision.is_some()).count();
    let score = if found.is_empty() {
        0.0
    } else {
        found.iter().map(|d| d.probability).sum::<f64>() / found.len() as f64
    };
    let applied = if decided > 0 && decider.applies() {
        "chosen"
    } else {
        "none"
    };
    decider
        .mark(&asked, asked.composed(json!({ "score": score })), applied)
        .await;
    asked
}

async fn ask_pending(decider: &Decider, units: &[Unit]) {
    let wanted = pending(units);
    let mut pace = Pace::new(MAX_CALLS);
    for group in wanted.chunks(GROUP) {
        if !pace.may_ask() {
            break;
        }
        let asked = ask_group(decider, group).await;
        pace.spent(&asked);
    }
}

fn decided_map(units: &[Unit]) -> HashMap<(i64, String), (i64, Decision)> {
    units
        .iter()
        .filter_map(|u| {
            let decision = MEMO.get(&(u.request_id, u.class)).and_then(|d| *d)?;
            Some(((u.turn_id, u.class.to_owned()), (u.request_id, decision)))
        })
        .collect()
}

pub async fn refine(
    env: Option<&DecisionEnv>,
    sources: &[DirectiveSourceRow],
    entries: Vec<NewDossierEntry>,
) -> Vec<NewDossierEntry> {
    let Some(decider) = env.and_then(|e| e.decider(POINT)) else {
        return entries;
    };
    let units = state::units(sources);
    ask_pending(&decider, &units).await;
    if !decider.applies() {
        return entries;
    }
    rewrite(entries, &decided_map(&units))
}
