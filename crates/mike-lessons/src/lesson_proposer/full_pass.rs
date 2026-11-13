use serde::Serialize;
use serde_json::Value;

use super::skill_author::AuthorPassReport;
use super::skill_eval::EvalPassReport;
use super::topics::TopicPassReport;
use super::{ProposerDeps, ProposerReport, run_tick, skill_author, skill_eval, topics};

#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct FullPass {
    pub proposer: ProposerReport,
    pub topics: TopicPassReport,
    pub skill_author: AuthorPassReport,
    pub skill_eval: EvalPassReport,
}

impl FullPass {
    pub fn counts(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

pub async fn run_full_pass(deps: &ProposerDeps<'_>) -> FullPass {
    let prompts = &deps.settings.prompts;
    let proposer = run_tick(deps).await;
    let topics = topics::run_pass(deps.persist, deps.llm, &prompts.topic, topics::BATCH).await;
    let skill_author = skill_author::run_pass(deps.persist, deps.llm, &prompts.skill_author).await;
    let skill_eval = skill_eval::run_pass(deps.persist, deps.llm, prompts).await;
    FullPass {
        proposer,
        topics,
        skill_author,
        skill_eval,
    }
}
