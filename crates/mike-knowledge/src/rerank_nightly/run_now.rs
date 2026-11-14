use chrono::Utc;
use serde_json::Value;

use super::{Idle, LEASE_ROLE, Pass, Stop, TICK_INTERVAL, night_pass, quiet_budget, report};
use crate::memory_workers::{self, RERANK_NIGHTLY, Skip};
use crate::semantic::KnowledgeState;
use crate::store::persistence::PersistHandle;

const MAX_PASSES: usize = 50;

#[derive(Debug, Clone, PartialEq)]
pub enum RunNow {
    Ran(Value),
    NoDatabase,
    TuningOff,
    OtherMachine,
}

struct OperatorAsked;

impl Idle for OperatorAsked {
    async fn is_idle(&self) -> bool {
        true
    }
}

pub async fn run_pass_now(state: &KnowledgeState) -> RunNow {
    let Some(persist) = state.persist().cloned() else {
        return RunNow::NoDatabase;
    };
    if !state.rerank_service().current_config().tuning_enabled {
        memory_workers::record_skip(RERANK_NIGHTLY, Utc::now(), Skip::Off);
        return RunNow::TuningOff;
    }
    if !persist.holds_worker_role(LEASE_ROLE, TICK_INTERVAL).await {
        memory_workers::record_skip(RERANK_NIGHTLY, Utc::now(), Skip::NotLeader);
        return RunNow::OtherMachine;
    }
    memory_workers::mark_running(RERANK_NIGHTLY, Utc::now());
    let total = passes(state, &persist).await;
    report(&total);
    let counts = total.counts();
    memory_workers::record_ran(RERANK_NIGHTLY, Utc::now(), &counts);
    RunNow::Ran(counts)
}

async fn passes(state: &KnowledgeState, persist: &PersistHandle) -> Pass {
    let budget = quiet_budget(state);
    let mut total = Pass::default();
    for _ in 0..MAX_PASSES {
        let pass = night_pass(state, persist, &budget, Utc::now(), &OperatorAsked).await;
        total.graded += pass.graded;
        total.thread_candidates += pass.thread_candidates;
        total.lesson_pairs += pass.lesson_pairs;
        total.tuned = pass.tuned.or(total.tuned);
        total.stopped = pass.stopped;
        if pass.stopped != Some(Stop::MoreToGrade) {
            break;
        }
    }
    total
}
