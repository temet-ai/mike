use async_trait::async_trait;
use serde_json::Value;

use super::{AnswerEngine, AnswerRequest, EngineError};
use crate::state::AppState;
use crate::wright::investigate::{
    self, EngineerContext, RunEnd, RunMode, RunRequest, StartRefused,
};

pub(crate) struct EngineerAnswers {
    state: AppState,
}

impl EngineerAnswers {
    pub(crate) fn for_state(state: &AppState) -> Self {
        Self {
            state: state.clone(),
        }
    }
}

fn refused(refusal: StartRefused) -> EngineError {
    match refusal {
        StartRefused::Busy | StartRefused::Overloaded(_) | StartRefused::DailyLimit { .. } => {
            EngineError::Busy(refusal.to_string())
        }
        StartRefused::Disabled | StartRefused::Unavailable(_) => {
            EngineError::NotAvailable(refusal.to_string())
        }
        StartRefused::Launch { .. } => EngineError::Failed(refusal.to_string()),
    }
}

fn settled(end: RunEnd, output: Result<Value, String>) -> Result<Value, EngineError> {
    match end {
        RunEnd::Killed { reason } => Err(EngineError::Failed(format!(
            "the answer run was stopped: {reason}"
        ))),
        RunEnd::Exited { .. } => output.map_err(EngineError::Failed),
    }
}

#[async_trait]
impl AnswerEngine for EngineerAnswers {
    async fn answer(&self, request: AnswerRequest) -> Result<Value, EngineError> {
        let ctx = EngineerContext::from_state(&self.state)
            .await
            .map_err(EngineError::NotAvailable)?;
        let run = RunRequest {
            mode: RunMode::Answer,
            problem_id: None,
            followup_id: None,
            brief: request.brief,
            inputs: request.inputs,
        };
        let handle = investigate::start_run(&ctx, run).await.map_err(refused)?;
        let outcome = investigate::wait(handle).await;
        settled(outcome.end, outcome.output)
    }

    async fn available(&self) -> bool {
        EngineerContext::from_state(&self.state).await.is_ok()
    }
}
