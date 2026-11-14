use tracing::warn;
use uuid::Uuid;

use super::verifier_judge::render_intake_criteria;
use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionSubject;
use crate::steerer::checklist_point::{Criterion, MAX_CRITERIA, intake_criteria, thread_criteria};
use crate::steerer::criteria_source_point;
use crate::steerer::verifier::build_accumulated_ask;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::acceptance_criteria::AcceptanceCriteriaMap;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::memory_ask_ledger::AskLedgerEntryRow;
use crate::store::persistence::steerer_repo::SteererPersist as _;
use mike_launch::store::persistence::work_threads::criteria::ThreadCriteria;

#[derive(Debug, Default, PartialEq)]
pub struct CriteriaFields {
    pub thread_criteria: Option<String>,
    pub intake_criteria: Option<String>,
    pub checklist_criteria: Vec<Criterion>,
}

pub struct CriteriaInputs {
    pub threads: Vec<ThreadCriteria>,
    pub intake: Option<AcceptanceCriteriaMap>,
}

fn thread_block(thread: &ThreadCriteria) -> String {
    let name = if thread.title.trim().is_empty() {
        &thread.thread_slug
    } else {
        &thread.title
    };
    format!("Work thread {name}:\n{}", thread.criteria_text.trim())
}

impl CriteriaInputs {
    pub fn new(threads: Vec<ThreadCriteria>, intake: Option<AcceptanceCriteriaMap>) -> Self {
        Self {
            threads,
            intake: intake.filter(|m| !m.criteria.is_empty()),
        }
    }

    pub fn fields(&self, accepted: &[usize]) -> CriteriaFields {
        let chosen: Vec<&ThreadCriteria> = accepted
            .iter()
            .filter_map(|i| self.threads.get(*i))
            .collect();
        if !chosen.is_empty() {
            let blocks: Vec<String> = chosen.iter().map(|t| thread_block(t)).collect();
            let checklist = chosen
                .iter()
                .flat_map(|t| thread_criteria(&t.criteria_text))
                .take(MAX_CRITERIA)
                .collect();
            return CriteriaFields {
                thread_criteria: Some(blocks.join("\n\n")),
                intake_criteria: None,
                checklist_criteria: checklist,
            };
        }
        self.intake
            .as_ref()
            .map_or_else(CriteriaFields::default, |map| CriteriaFields {
                thread_criteria: None,
                intake_criteria: Some(render_intake_criteria(map)),
                checklist_criteria: intake_criteria(map),
            })
    }

    pub fn by_default(&self) -> CriteriaFields {
        self.fields(&[])
    }

    pub async fn picked(
        &self,
        decider: Option<&Decider>,
        ask_entries: &[AskLedgerEntryRow],
        subject: DecisionSubject,
    ) -> CriteriaFields {
        let asks = build_accumulated_ask(ask_entries);
        let accepted =
            criteria_source_point::accepted(decider, &self.threads, &asks, subject).await;
        self.fields(&accepted)
    }
}

pub async fn load_criteria_inputs(
    ph: &PersistHandle,
    session_uuid: Uuid,
    session_db_id: Option<i64>,
) -> CriteriaInputs {
    let Some(session_db_id) = session_db_id else {
        return CriteriaInputs::new(Vec::new(), None);
    };
    let threads = ph
        .launch()
        .fetch_linked_thread_criteria(session_db_id)
        .await
        .unwrap_or_else(|e| {
            warn!(session_id = %session_uuid, error = %e, "steerer fleet: linked work-thread criteria lookup failed");
            Vec::new()
        });
    let intake = ph
        .steerer()
        .fetch_current_acceptance_criteria(session_db_id)
        .await
        .unwrap_or_else(|e| {
            warn!(session_id = %session_uuid, error = %e, "steerer fleet: intake-criteria lookup failed");
            None
        });
    CriteriaInputs::new(threads, intake)
}

#[cfg(test)]
#[path = "verifier_criteria_tests.rs"]
mod tests;
