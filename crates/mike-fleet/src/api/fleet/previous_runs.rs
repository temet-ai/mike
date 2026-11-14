use crate::store::persistence::launch_repo::LaunchPersist as _;
use serde::Serialize;

use crate::launch::fleet::FleetDefinition;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::work_threads::{HistoryEntry, HistoryPage};

const SHOWN: usize = 5;
const WINDOW: i64 = 50;
const FINDING: &str = "finding";
const ASK: &str = "Say in your answer what this definition changes because of these findings, or why a finding does not apply.";

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct PreviousFinding {
    pub entry: String,
    pub summary: String,
    pub at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct PreviousRuns {
    pub thread: String,
    pub findings: Vec<PreviousFinding>,
    pub ask: &'static str,
}

pub fn newest(thread: &str, mut history: Vec<HistoryEntry>) -> Option<PreviousRuns> {
    history.retain(|entry| entry.kind == FINDING);
    history.sort_by_key(|entry| std::cmp::Reverse(entry.ord));
    let findings: Vec<PreviousFinding> = history
        .into_iter()
        .take(SHOWN)
        .map(|entry| PreviousFinding {
            entry: format!("{thread}#{}", entry.ord),
            summary: entry.summary,
            at: entry.created_at,
        })
        .collect();
    (!findings.is_empty()).then(|| PreviousRuns {
        thread: thread.to_string(),
        findings,
        ask: ASK,
    })
}

pub async fn load(
    persist: &PersistHandle,
    project_path: &str,
    def: &FleetDefinition,
) -> Option<PreviousRuns> {
    let thread = def.retrospective.thread.as_str();
    let project = persist
        .launch()
        .resolve_work_thread_project(project_path)
        .await
        .ok()??;
    let detail = persist
        .launch()
        .get_work_thread(
            project.id,
            thread,
            HistoryPage {
                limit: WINDOW,
                before_ord: None,
            },
        )
        .await
        .ok()?;
    newest(thread, detail.history)
}

#[cfg(test)]
#[path = "previous_runs_tests.rs"]
mod tests;
