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
    let (persist, llm, decisions) = (deps.persist, deps.llm, deps.decisions);
    let topics = topics::run_pass(persist, llm, decisions, &prompts.topic, topics::BATCH).await;
    let skill_author = skill_author::run_pass(persist, llm, &prompts.skill_author).await;
    let calls = skill_eval::EvalCalls { prompts, decisions };
    let skill_eval = skill_eval::run_pass_with(persist, llm, calls).await;
    FullPass {
        proposer,
        topics,
        skill_author,
        skill_eval,
    }
}
