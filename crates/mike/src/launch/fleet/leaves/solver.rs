use serde::Serialize;
use serde_json::json;

use super::SOLVER_PROMPT;
use crate::launch::fleet::events::FleetEvent;
use crate::launch::fleet::problems::{
    DECISIONS_PER_PROBLEM, DecisionKind, Ledger, MINUTES_PER_PROBLEM, Problem, TURNS_PER_PROBLEM,
    Trigger,
};

const TRAIL_EVENTS: usize = 15;
const OTHER_TASKS: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskTrail {
    pub task: String,
    pub recent: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Dossier {
    pub problem_id: String,
    pub trigger: String,
    pub detail: String,
    pub plan_version: Option<i64>,
    pub tasks: Vec<TaskTrail>,
    pub earlier_decisions: Vec<String>,
    pub lessons: Vec<String>,
    pub allowed_decisions: Vec<&'static str>,
    pub turns: u32,
    pub minutes: i64,
    pub decisions_before_the_operator: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolverBrief {
    pub model: &'static str,
    pub text: String,
}

fn line(event: &FleetEvent) -> String {
    format!(
        "{} {} {}",
        event.occurred_at.format("%H:%M:%S"),
        event.kind.as_str(),
        event.payload
    )
}

fn concerned(problem: &Problem, events: &[FleetEvent]) -> Vec<String> {
    if let Some(task) = &problem.task {
        return vec![task.clone()];
    }
    let mut tasks: Vec<String> = Vec::new();
    for task in events.iter().rev().filter_map(|e| e.task.clone()) {
        if !tasks.contains(&task) && tasks.len() < OTHER_TASKS {
            tasks.push(task);
        }
    }
    tasks
}

fn trail(task: &str, events: &[FleetEvent]) -> TaskTrail {
    let mut mine: Vec<&FleetEvent> = events
        .iter()
        .filter(|e| e.task.as_deref() == Some(task))
        .collect();
    mine.sort_by_key(|e| (e.occurred_at, e.id));
    let from = mine.len().saturating_sub(TRAIL_EVENTS);
    TaskTrail {
        task: task.to_string(),
        recent: mine[from..].iter().map(|e| line(e)).collect(),
    }
}

fn plan_version(events: &[FleetEvent]) -> Option<i64> {
    events
        .iter()
        .filter(|e| e.kind.as_str() == "plan_versioned")
        .filter_map(|e| e.payload.get("version").and_then(serde_json::Value::as_i64))
        .max()
}

pub fn dossier(
    problem: &Problem,
    detail: &str,
    events: &[FleetEvent],
    ledger: &Ledger,
    lessons: &[String],
) -> Dossier {
    let earlier = ledger
        .problems()
        .iter()
        .filter(|p| p.id != problem.id)
        .flat_map(|p| {
            p.decisions
                .iter()
                .map(|d| format!("{} {}: {}", p.trigger, p.task.as_deref().unwrap_or("-"), d))
        })
        .collect();
    Dossier {
        problem_id: problem.id.to_string(),
        trigger: problem.trigger.clone(),
        detail: detail.to_string(),
        plan_version: plan_version(events),
        tasks: concerned(problem, events)
            .iter()
            .map(|task| trail(task, events))
            .collect(),
        earlier_decisions: earlier,
        lessons: lessons.to_vec(),
        allowed_decisions: DecisionKind::names(),
        turns: TURNS_PER_PROBLEM,
        minutes: MINUTES_PER_PROBLEM,
        decisions_before_the_operator: DECISIONS_PER_PROBLEM,
    }
}

pub fn brief(dossier: &Dossier) -> SolverBrief {
    let model = match Trigger::parse(&dossier.trigger) {
        Some(trigger) if trigger.needs_opus() => "opus",
        _ => "sonnet",
    };
    let body = serde_json::to_string_pretty(&json!({ "dossier": dossier })).unwrap_or_default();
    SolverBrief {
        model,
        text: format!("{SOLVER_PROMPT}\n\n{body}\n"),
    }
}
