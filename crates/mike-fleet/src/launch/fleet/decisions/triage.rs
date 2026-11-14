use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::config::{Mode, Point};
use super::triage_rules::{Action, Triage, by_rules};
use super::{FleetDecisions, text};
use crate::llm::decisions::Question;
use crate::llm::decisions::point::{Asked, choice};
use crate::llm::decisions::record::DecisionSubject;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::TreeCheckRow;
use crate::store::persistence::fleet_repo::FleetPersist as _;

pub const QUESTION_SET: &str = "fleet_triage_v1";
pub const OUTPUT_TAIL_BYTES: usize = 4000;
const NEXT_STEP: &str = "next_step";

pub fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([(
        NEXT_STEP.to_owned(),
        choice(
            "What should the seat do about this failed check?",
            &[
                (
                    "retry",
                    "The failure looks transient: a flaky test, a network blip, a lock or a resource shortage. Running it again may pass without any change.",
                ),
                (
                    "fix_code",
                    "The output shows a defect in the code under test: a failing assertion, a compile error, a type error.",
                ),
                (
                    "fix_env",
                    "The output shows a problem outside the code: a missing tool, module or file, a permissions problem, a full disk, a service that is down.",
                ),
            ],
        ),
    )])
}

pub fn state(row: &TreeCheckRow) -> Value {
    json!({
        "command": row.command,
        "exit_code": row.exit_code,
        "output_tail": text::tail(&row.output, OUTPUT_TAIL_BYTES),
    })
}

pub fn compose(asked: &Asked) -> Option<(Triage, f64)> {
    if asked.failed() {
        return None;
    }
    let (key, probability) = asked.choice(NEXT_STEP)?;
    let action = Action::from_key(key)?;
    Some((
        Triage {
            action,
            reason: format!(
                "Jev's reading of the output, {:.0}% sure",
                probability * 100.0
            ),
            by: "jev",
        },
        probability,
    ))
}

async fn jev_step(
    decisions: Option<&FleetDecisions>,
    mode: Mode,
    row: &TreeCheckRow,
) -> Option<Triage> {
    let decider = decisions?.decider(Point::Triage, mode)?;
    let subject = DecisionSubject {
        check_receipt_id: Some(row.id),
        ..DecisionSubject::default()
    };
    let asked = decider
        .ask(QUESTION_SET, state(row), questions(), subject, json!({}))
        .await;
    let composed = compose(&asked);
    let score = composed.as_ref().map_or(1.0, |(_, p)| *p);
    let action = composed.as_ref().map(|(triage, _)| triage.action.key());
    let verdict = json!({"score": score, "triage": action});
    if !decider.applies() {
        decider.mark(&asked, asked.composed(verdict), "none").await;
        return None;
    }
    let applied = if composed.is_some() { "chosen" } else { "none" };
    decider.mark(&asked, asked.composed(verdict), applied).await;
    composed.map(|(triage, _)| triage)
}

pub async fn triage(
    decisions: Option<&FleetDecisions>,
    mode: Mode,
    row: &TreeCheckRow,
) -> Option<Triage> {
    if row.finished_at.is_none() || row.passed() {
        return None;
    }
    if let Some(found) = by_rules(row.exit_code, &row.output) {
        return Some(found);
    }
    jev_step(decisions, mode, row).await
}

pub async fn store(
    persist: &PersistHandle,
    decisions: Option<&FleetDecisions>,
    mode: Mode,
    row: &TreeCheckRow,
) {
    let Some(found) = triage(decisions, mode, row).await else {
        return;
    };
    if let Err(error) = persist
        .fleet()
        .set_tree_check_triage(row.id, &found.line())
        .await
    {
        tracing::warn!(%error, receipt = row.id, "fleet: could not store the check triage");
    }
}

#[cfg(test)]
#[path = "triage_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "triage_db_tests.rs"]
mod db_tests;
