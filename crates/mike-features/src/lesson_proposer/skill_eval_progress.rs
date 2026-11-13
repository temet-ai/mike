use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::catalog::RealSkills;
use super::review::{Decision, Review};
use super::trigger::{QueryResult, batch_count};
use super::workspace::Workspace;
use super::{EVALUATOR_VERSION, EvalError, Material, PARALLEL_CALLS};
use crate::store::persistence::lessons::skill_draft_eval::SiblingSkill;

pub const BATCH_CALLS: u32 = 1;
pub const REVIEW_CALLS: u32 = 1;

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    #[serde(default)]
    pub evaluator_version: i32,
    pub siblings: Option<Vec<SiblingSkill>>,
    #[serde(default)]
    pub real_skills: Option<RealSkills>,
    pub models: Vec<String>,
    pub trigger: Vec<QueryResult>,
    #[serde(default)]
    pub trigger_batches: usize,
    #[serde(default)]
    pub workspace: Option<Workspace>,
    #[serde(default)]
    pub review: Option<Review>,
    #[serde(default)]
    pub recheck: Vec<QueryResult>,
    #[serde(default)]
    pub recheck_batches: usize,
    #[serde(default)]
    pub calls: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Batch(usize),
    Review,
    Recheck(usize),
}

impl Unit {
    pub fn cost(self) -> u32 {
        match self {
            Self::Batch(_) | Self::Recheck(_) => BATCH_CALLS,
            Self::Review => REVIEW_CALLS,
        }
    }
}

impl Progress {
    pub fn fresh() -> Self {
        Self {
            evaluator_version: EVALUATOR_VERSION,
            ..Self::default()
        }
    }

    pub fn from_row(value: Option<&Value>) -> Self {
        value.and_then(Self::saved).unwrap_or_else(Self::fresh)
    }

    fn saved(value: &Value) -> Option<Self> {
        match serde_json::from_value::<Self>(value.clone()) {
            Ok(saved) if saved.evaluator_version == EVALUATOR_VERSION => Some(saved),
            Ok(saved) => {
                log_other_version(saved.evaluator_version);
                None
            }
            Err(e) => {
                log_unreadable(&e);
                None
            }
        }
    }

    pub fn patched(&self) -> bool {
        self.review
            .as_ref()
            .is_some_and(|r| r.decision == Decision::Patch)
    }

    pub fn pending(&self, material: &Material) -> Vec<Unit> {
        let total = batch_count(material.queries.len());
        if self.trigger_batches < total {
            return (self.trigger_batches..total).map(Unit::Batch).collect();
        }
        if self.review.is_none() {
            return vec![Unit::Review];
        }
        if self.patched() && self.recheck_batches < total {
            return (self.recheck_batches..total).map(Unit::Recheck).collect();
        }
        Vec::new()
    }

    pub fn worst_case_calls(&self, material: &Material) -> u32 {
        let total = batch_count(material.queries.len());
        let first = total.saturating_sub(self.trigger_batches);
        let unreviewed = self.review.is_none();
        let again = if unreviewed || self.patched() {
            total.saturating_sub(self.recheck_batches)
        } else {
            0
        };
        let batches = u32::try_from(first + again).unwrap_or(u32::MAX);
        batches.saturating_mul(BATCH_CALLS) + u32::from(unreviewed) * REVIEW_CALLS
    }

    pub fn note_model(&mut self, model: &str) {
        if !self.models.iter().any(|m| m == model) {
            self.models.push(model.to_owned());
        }
    }
}

fn log_other_version(saved: i32) {
    tracing::info!(
        saved,
        current = EVALUATOR_VERSION,
        "skill eval: saved progress belongs to another evaluator version, starting over"
    );
}

fn log_unreadable(error: &serde_json::Error) {
    tracing::warn!(%error, "skill eval: saved progress unreadable, starting over");
}

pub fn plan(pending: &[Unit], granted: u32) -> Vec<Unit> {
    let mut spent = 0_u32;
    pending
        .iter()
        .copied()
        .take_while(|unit| {
            spent = spent.saturating_add(unit.cost());
            spent <= granted
        })
        .collect()
}

pub fn cost(plan: &[Unit]) -> u32 {
    plan.iter().map(|u| u.cost()).sum()
}

pub fn settled<T>(results: Vec<Result<T, EvalError>>) -> (Vec<T>, Option<EvalError>) {
    let mut done = Vec::new();
    for result in results {
        match result {
            Ok(item) => done.push(item),
            Err(error) => return (done, Some(error)),
        }
    }
    (done, None)
}

pub fn batch_ids(plan: &[Unit]) -> Vec<usize> {
    plan.iter()
        .filter_map(|unit| match *unit {
            Unit::Batch(i) | Unit::Recheck(i) => Some(i),
            Unit::Review => None,
        })
        .collect()
}

pub async fn in_chunks<F: std::future::Future>(mut tasks: Vec<F>) -> Vec<F::Output> {
    let mut out = Vec::with_capacity(tasks.len());
    while !tasks.is_empty() {
        let rest = tasks.split_off(PARALLEL_CALLS.min(tasks.len()));
        let chunk = std::mem::replace(&mut tasks, rest);
        out.extend(futures::future::join_all(chunk).await);
    }
    out
}
