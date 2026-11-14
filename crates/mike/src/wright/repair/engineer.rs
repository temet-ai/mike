use super::port::{RepairEngineer, RepairJob, RepairRun};
use crate::state::AppState;
use crate::wright::investigate::{self, EngineerContext, RunEnd, RunMode, RunRequest};

pub(crate) struct InvestigateEngineer {
    state: AppState,
}

impl InvestigateEngineer {
    pub(crate) fn for_state(state: &AppState) -> Self {
        Self {
            state: state.clone(),
        }
    }
}

fn not_started(reason: String) -> RepairRun {
    RepairRun {
        not_started: Some(reason),
        failure: None,
    }
}

fn ended(end: &RunEnd, output: &Result<serde_json::Value, String>) -> RepairRun {
    let failure = match (end, output) {
        (RunEnd::Killed { reason }, _) => Some(format!("the repair run was stopped: {reason}")),
        (RunEnd::Exited { code }, _) if *code != Some(0) => {
            Some(format!("the repair run exited with {code:?}"))
        }
        (_, Err(reason)) => Some(format!("the repair run left no valid output: {reason}")),
        _ => None,
    };
    RepairRun {
        not_started: None,
        failure,
    }
}

impl RepairEngineer for InvestigateEngineer {
    async fn run_in_worktree(&self, job: &RepairJob) -> RepairRun {
        tracing::info!(
            repair = job.repair_id,
            problem = job.problem_id,
            finding = job.finding_id,
            worktree = %job.worktree.display(),
            "wright repair: starting the engineer run"
        );
        let ctx = match EngineerContext::from_state(&self.state).await {
            Ok(ctx) => ctx,
            Err(reason) => return not_started(reason),
        };
        let request = RunRequest {
            mode: RunMode::Repair {
                worktree: job.worktree.clone(),
            },
            problem_id: Some(job.problem_id),
            brief: job.brief.clone(),
            inputs: job.inputs.clone(),
        };
        match investigate::start_run(&ctx, request).await {
            Ok(handle) => {
                let outcome = investigate::wait(handle).await;
                ended(&outcome.end, &outcome.output)
            }
            Err(refused) => not_started(refused.to_string()),
        }
    }
}
