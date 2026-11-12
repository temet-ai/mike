use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::definition_view::DefinitionError;
use super::tree::running;
use crate::launch::fleet::problems::{self, Decision, DecisionKind};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub(crate) struct DecideRequest {
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
pub(crate) struct DecideAnswer {
    problem_id: Uuid,
    recorded: Vec<String>,
    escalated: bool,
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

pub(crate) async fn decide_problem(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(req): Json<DecideRequest>,
) -> Result<Json<DecideAnswer>, DefinitionError> {
    let (_row, _def, run) = running(&state, &key).await?;
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
    Ok(Json(DecideAnswer {
        problem_id: req.problem_id,
        recorded: kinds,
        escalated,
    }))
}
