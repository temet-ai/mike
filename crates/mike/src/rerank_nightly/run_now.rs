use chrono::Utc;
use serde_json::{Value, json};

use super::{Idle, LEASE_ROLE, Pass, Stop, TICK_INTERVAL, night_pass, quiet_budget, report};
use crate::config::AppState;
use crate::store::persistence::PersistHandle;

const MAX_PASSES: usize = 50;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RunNow {
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

pub(crate) async fn run_pass_now(state: &AppState) -> RunNow {
    let Some(persist) = state.persist().cloned() else {
        return RunNow::NoDatabase;
    };
    if !state.rerank_service().current_config().tuning_enabled {
        return RunNow::TuningOff;
    }
    if !persist.holds_worker_role(LEASE_ROLE, TICK_INTERVAL).await {
        return RunNow::OtherMachine;
    }
    let total = passes(state, &persist).await;
    report(&total);
    RunNow::Ran(pass_json(&total))
}

async fn passes(state: &AppState, persist: &PersistHandle) -> Pass {
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

fn pass_json(pass: &Pass) -> Value {
    json!({
        "graded": pass.graded,
        "tuned": pass.tuned.map(|tuned| format!("{tuned:?}")),
        "thread_candidates": pass.thread_candidates,
        "lesson_pairs": pass.lesson_pairs,
        "stopped": pass.stopped.map(|stop| format!("{stop:?}")),
    })
}
