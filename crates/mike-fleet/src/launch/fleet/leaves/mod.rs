pub mod checker;
pub mod judge;
pub mod judge_jev;
pub mod judge_pass;
#[cfg(test)]
pub mod judge_pass_tests;
pub mod pass;
pub mod report_judge;
#[cfg(test)]
pub mod report_judge_tests;
pub mod solver;
pub mod solver_jev;
#[cfg(test)]
mod tests;

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::base_state::BaseState;
use crate::llm::OpenRouterClient;

pub use checker::{Checked, CheckerAnswer, CheckerQuestion, check};
pub use judge::{
    CriterionText, EarlierVerdict, JudgeAnswer, JudgeInput, JudgeVerdict, Judged, ReportFacts,
    SharedTask, judge, verdict_events,
};
pub use judge_pass::pass as judge_reports;
pub use pass::pass;
pub use pass::{ModelSolver, Solver, decisions_for};
pub use report_judge::{
    FAILS_BEFORE_A_PROBLEM, ReportToJudge, earlier_rounds, judge_report, repeatedly_failed,
    tell_seat,
};
pub use solver::{Dossier, SolverBrief, TaskTrail, brief, dossier};

pub fn model(state: &BaseState) -> Option<OpenRouterLeaf> {
    let config = &state.config.steerer;
    let base = OpenRouterClient::new(
        String::new(),
        config.model.clone(),
        Some(config.openrouter_base_url.clone()),
        Some(std::time::Duration::from_secs(config.request_timeout_secs)),
    )
    .ok()?
    .with_attribution(
        crate::llm::LlmRole::JudgeWorker,
        state.llm_usage_sink.as_ref(),
    )
    .with_live_settings(crate::settings::steerer_live_source(
        &state.settings_overlay,
    ));
    crate::llm::key::effective_client(&base, &state.steerer_openrouter_key).map(OpenRouterLeaf::new)
}
pub const JUDGE_PROMPT: &str = include_str!("../../../../../../.claude/fleet/leaves/judge.md");
pub const CHECKER_PROMPT: &str = include_str!("../../../../../../.claude/fleet/leaves/checker.md");
pub const SOLVER_PROMPT: &str =
    include_str!("../../../../../../.claude/fleet/leaves/problem-solver.md");

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LeafError {
    #[error("the call was refused: {0}")]
    Refused(String),
    #[error("the model call failed: {0}")]
    Transport(String),
    #[error("the answer does not fit the schema: {0}")]
    Invalid(String),
    #[error("the judge is called only after the floor passed; the floor at {sha} did not")]
    FloorNotPassed { sha: String },
}

#[async_trait]
pub trait LeafModel: Send + Sync {
    async fn answer(&self, system: &str, user: &str, schema: Value) -> Result<Value, LeafError>;
}

pub struct OpenRouterLeaf {
    client: OpenRouterClient,
}

impl OpenRouterLeaf {
    pub fn new(client: OpenRouterClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl LeafModel for OpenRouterLeaf {
    async fn answer(&self, system: &str, user: &str, schema: Value) -> Result<Value, LeafError> {
        let result = self
            .client
            .verdict_with_usage::<Value>(system, user, schema)
            .await;
        match result {
            Ok((value, _usage)) => Ok(value),
            Err(err) if err.is_refusal() => Err(LeafError::Refused(err.to_string())),
            Err(err) => Err(LeafError::Transport(err.to_string())),
        }
    }
}

pub fn parse<T: DeserializeOwned>(answer: Value) -> Result<T, LeafError> {
    serde_json::from_value(answer).map_err(|err| LeafError::Invalid(err.to_string()))
}

pub fn require_text(field: &str, value: &str) -> Result<(), LeafError> {
    if value.trim().is_empty() {
        return Err(LeafError::Invalid(format!("{field} is empty")));
    }
    Ok(())
}
