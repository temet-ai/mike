use crate::store::persistence::fleet_repo::FleetPersist as _;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Dossier, LeafError, LeafModel, brief, dossier, parse, require_text};
use crate::launch::fleet::definition::FleetDefinition;
use crate::launch::fleet::events::{FleetEvent, NewFleetEvent};
use crate::launch::fleet::problems::{
    Decision, DecisionKind, Ledger, decide_events, overdue_events,
};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;

#[async_trait]
pub trait Solver: Send + Sync {
    async fn solve(&self, dossier: &Dossier) -> Result<Decision, LeafError>;
}

#[derive(Debug, Deserialize)]
struct SolverAnswer {
    decision: String,
    reason: String,
    evidence_read: Vec<String>,
    #[serde(default)]
    question: Option<String>,
}

pub struct ModelSolver<M: LeafModel>(pub M);

pub fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["decision", "reason", "evidence_read", "question"],
        "properties": {
            "decision": { "type": "string", "enum": DecisionKind::names() },
            "reason": { "type": "string" },
            "evidence_read": { "type": "array", "items": { "type": "string" } },
            "question": { "type": ["string", "null"] }
        }
    })
}

#[async_trait]
impl<M: LeafModel> Solver for ModelSolver<M> {
    async fn solve(&self, dossier: &Dossier) -> Result<Decision, LeafError> {
        let brief = brief(dossier);
        let raw = self
            .0
            .answer(
                "You are the problem-solver of a fleet run.",
                &brief.text,
                schema(),
            )
            .await?;
        let answer: SolverAnswer = parse(raw)?;
        require_text("reason", &answer.reason)?;
        let kind = DecisionKind::parse(&answer.decision)
            .ok_or_else(|| LeafError::Invalid(format!("unknown decision {}", answer.decision)))?;
        Ok(Decision {
            kind,
            decided_by: "problem_solver".into(),
            reason: answer.reason,
            evidence_read: answer.evidence_read,
            question: answer.question,
        })
    }
}

pub async fn decisions_for(
    events: &[FleetEvent],
    run: &str,
    now: DateTime<Utc>,
    solver: &dyn Solver,
    lessons: &[String],
) -> Vec<NewFleetEvent> {
    let ledger = Ledger::from_events(events);
    let mut out = overdue_events(&ledger, run, now);
    for problem in ledger.problems().iter().filter(|p| p.is_active()) {
        if is_overdue(problem, now) {
            continue;
        }
        let detail = detail_of(events, problem.id.to_string().as_str());
        let brief = dossier(problem, &detail, events, &ledger, lessons);
        let Ok(decision) = solver.solve(&brief).await else {
            continue;
        };
        if let Ok(new) = decide_events(&ledger, run, problem.id, &decision, now) {
            out.extend(new);
        }
    }
    out
}

fn is_overdue(problem: &crate::launch::fleet::problems::Problem, now: DateTime<Utc>) -> bool {
    (now - problem.opened_at).num_minutes() > crate::launch::fleet::problems::MINUTES_PER_PROBLEM
}

fn detail_of(events: &[FleetEvent], problem_id: &str) -> String {
    events
        .iter()
        .find(|e| e.payload.get("problem_id").and_then(Value::as_str) == Some(problem_id))
        .and_then(|e| e.payload.get("detail").and_then(Value::as_str))
        .unwrap_or_default()
        .to_string()
}

pub async fn pass(
    state: &AppState,
    persist: &PersistHandle,
    _def: &FleetDefinition,
    run: &str,
) {
    let Ok(events) = persist.fleet().fleet_events_since(run, 0).await else {
        return;
    };
    if !Ledger::from_events(&events)
        .problems()
        .iter()
        .any(|p| p.is_active())
    {
        return;
    }
    let Some(model) = super::model(state) else {
        return;
    };
    let solver = ModelSolver(model);
    let new = decisions_for(&events, run, Utc::now(), &solver, &[]).await;
    for event in new {
        if let Err(err) = persist.fleet().append_fleet_event(&event).await {
            tracing::warn!(run, error = %err, "problem pass could not record an event");
            return;
        }
    }
}
