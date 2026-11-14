use super::port::{RepairEngineer, RepairJob, RepairRun};

pub(crate) struct InvestigateEngineer;

impl InvestigateEngineer {
    pub(crate) fn for_state(_state: &crate::state::AppState) -> Self {
        Self
    }
}

impl RepairEngineer for InvestigateEngineer {
    async fn run_in_worktree(&self, job: &RepairJob) -> RepairRun {
        RepairRun {
            not_started: Some(format!(
                "repair {} (problem {}, finding {}) needs an engineer run in {} with a {}-byte brief and {} inputs, and engineer runs (temet-ai/mike#73) are not built into this mike",
                job.repair_id,
                job.problem_id,
                job.finding_id,
                job.worktree.display(),
                job.brief.len(),
                job.inputs.len()
            )),
            failure: None,
        }
    }
}
