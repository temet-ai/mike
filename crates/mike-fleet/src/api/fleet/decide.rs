use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::definition_view::DefinitionError;
use super::tree::running;
use crate::base_state::BaseState;
use crate::launch::fleet::operator_route;
use crate::launch::fleet::problems::{self, Decision, DecisionKind};

#[derive(Debug, Deserialize)]
pub struct DecideRequest {
    pub session_id: Option<Uuid>,
    pub problem_id: Uuid,
    pub decision: String,
    pub reason: String,
    #[serde(default)]
    pub evidence_read: Vec<String>,
    #[serde(default)]
    pub question: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DecideAnswer {
    problem_id: Uuid,
    recorded: Vec<String>,
    escalated: bool,
    routed: bool,
}

fn decision_of(req: &DecideRequest) -> Result<Decision, DefinitionError> {
    let kind = DecisionKind::parse(&req.decision).ok_or_else(|| {
        DefinitionError::InvalidRequest(format!(
            "decision `{}` is not one of {}",
            req.decision,
            DecisionKind::names().join(", ")
        ))
    })?;
    Ok(Decision {
        kind,
        decided_by: req
            .session_id
            .map_or_else(|| "problem_solver".to_string(), |id| id.to_string()),
        reason: req.reason.clone(),
        evidence_read: req.evidence_read.clone(),
        question: req.question.clone(),
    })
}

pub async fn decide_problem(
    State(state): State<BaseState>,
    Path(key): Path<String>,
    Json(req): Json<DecideRequest>,
) -> Result<Json<DecideAnswer>, DefinitionError> {
    let (_row, def, run) = running(&state, &key).await?;
    let persist = state.persist().ok_or(DefinitionError::Unavailable)?;
    let decision = decision_of(&req)?;
    let events = problems::decide(persist, &run, req.problem_id, &decision)
        .await
        .map_err(DefinitionError::InvalidRequest)?;
    let kinds: Vec<String> = events
        .iter()
        .filter_map(|e| e.payload.kind().ok())
        .map(|k| k.as_str().to_string())
        .collect();
    let escalated = kinds.iter().any(|k| k == "problem_escalated");
    let routed = escalated && route_to_coordinator(&state, &def, &run, &decision, &req).await;
    Ok(Json(DecideAnswer {
        problem_id: req.problem_id,
        recorded: kinds,
        escalated,
        routed,
    }))
}

async fn route_to_coordinator(
    state: &BaseState,
    def: &crate::launch::fleet::definition::FleetDefinition,
    run: &str,
    decision: &Decision,
    req: &DecideRequest,
) -> bool {
    let (Some(question), Some(persist)) = (decision.question.as_deref(), state.persist()) else {
        return false;
    };
    if decision.kind != DecisionKind::Escalate {
        return false;
    }
    let Some(decider) = state
        .decision_env()
        .and_then(|env| env.decider(operator_route::POINT))
    else {
        return false;
    };
    let escalation = operator_route::Escalation {
        run,
        problem_id: req.problem_id,
        task: None,
        raised_by: &decision.decided_by,
        question,
        reason: &decision.reason,
    };
    operator_route::route_escalation(&decider, persist, def, &escalation).await
}
