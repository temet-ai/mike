use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};

use super::{Paged, clamp_limit, clamp_offset};
use crate::api::handlers::lesson_error;
use crate::state::AppState;
use crate::store::persistence::lesson_relations::{LessonRelationRow, RelationDecisionOutcome};
use crate::web_base::{persist_error_response, persist_unavailable_response};

#[derive(Debug, Serialize)]
pub(crate) struct LessonRelationDto {
    pub(crate) id: i64,
    pub(crate) uuid: String,
    pub(crate) from_lesson_id: i64,
    pub(crate) from_lesson_text: String,
    pub(crate) from_lesson_created_at: String,
    pub(crate) to_lesson_id: i64,
    pub(crate) to_lesson_text: String,
    pub(crate) to_lesson_created_at: String,
    pub(crate) kind: String,
    pub(crate) method: String,
    pub(crate) status: String,
    pub(crate) candidate_distance: Option<f64>,
    pub(crate) evidence_json: serde_json::Value,
    pub(crate) proposer_model: Option<String>,
    pub(crate) proposer_rationale: Option<String>,
    pub(crate) adjudicator_model: Option<String>,
    pub(crate) adjudicator_agrees: Option<bool>,
    pub(crate) adjudicator_rationale: Option<String>,
    pub(crate) detector_version: String,
    pub(crate) proposed_at: String,
    pub(crate) decided_at: Option<String>,
    pub(crate) decided_by: Option<String>,
    pub(crate) created_at: String,
}

impl From<LessonRelationRow> for LessonRelationDto {
    fn from(r: LessonRelationRow) -> Self {
        Self {
            id: r.id,
            uuid: r.uuid.to_string(),
            from_lesson_id: r.from_lesson_id,
            from_lesson_text: r.from_lesson_text,
            from_lesson_created_at: r.from_lesson_created_at.to_rfc3339(),
            to_lesson_id: r.to_lesson_id,
            to_lesson_text: r.to_lesson_text,
            to_lesson_created_at: r.to_lesson_created_at.to_rfc3339(),
            kind: r.kind,
            method: r.method,
            status: r.status,
            candidate_distance: r.candidate_distance,
            evidence_json: r.evidence_json,
            proposer_model: r.proposer_model,
            proposer_rationale: r.proposer_rationale,
            adjudicator_model: r.adjudicator_model,
            adjudicator_agrees: r.adjudicator_agrees,
            adjudicator_rationale: r.adjudicator_rationale,
            detector_version: r.detector_version,
            proposed_at: r.proposed_at.to_rfc3339(),
            decided_at: r.decided_at.map(|t| t.to_rfc3339()),
            decided_by: r.decided_by,
            created_at: r.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct LessonRelationsListParams {
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) kind: Option<String>,
    #[serde(default)]
    pub(crate) status: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<i64>,
    #[serde(default)]
    pub(crate) offset: Option<i64>,
}

pub(crate) async fn list_lesson_relations(
    State(state): State<AppState>,
    Query(params): Query<LessonRelationsListParams>,
) -> Result<Json<Paged<LessonRelationDto>>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let filter = crate::store::persistence::lesson_relations::LessonRelationsFilter {
        project: params.project.filter(|p| !p.trim().is_empty()),
        kind: params.kind,
        status: params.status,
        limit: clamp_limit(params.limit),
        offset: clamp_offset(params.offset),
    };
    let (total_count, rows) = handle.list_relations(&filter).await.map_err(|e| {
        tracing::warn!(error = %e, "list_lesson_relations: query failed");
        persist_error_response(e)
    })?;
    Ok(Json(Paged {
        items: rows.into_iter().map(LessonRelationDto::from).collect(),
        total_count,
        limit: filter.limit,
        offset: filter.offset,
    }))
}

enum RelationDecisionError {
    NotFound,
    WrongStatus(String),
}

fn relation_decision_dto(
    outcome: RelationDecisionOutcome,
) -> Result<LessonRelationDto, RelationDecisionError> {
    match outcome {
        RelationDecisionOutcome::NotFound => Err(RelationDecisionError::NotFound),
        RelationDecisionOutcome::WrongStatus(status) => {
            Err(RelationDecisionError::WrongStatus(status))
        }
        RelationDecisionOutcome::Decided(row) => Ok(LessonRelationDto::from(*row)),
    }
}

fn relation_decision_error_response(error: RelationDecisionError) -> Response {
    match error {
        RelationDecisionError::NotFound => {
            lesson_error(StatusCode::NOT_FOUND, "relation not found")
        }
        RelationDecisionError::WrongStatus(status) => lesson_error(
            StatusCode::CONFLICT,
            &format!("relation is already {status}"),
        ),
    }
}

pub(crate) async fn confirm_lesson_relation(
    State(state): State<AppState>,
    Path(relation_id): Path<i64>,
) -> Result<Json<LessonRelationDto>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let outcome = handle
        .confirm_relation(relation_id, "operator")
        .await
        .map_err(|e| {
            tracing::warn!(relation_id, error = %e, "confirm_lesson_relation: query failed");
            persist_error_response(e)
        })?;
    relation_decision_dto(outcome)
        .map(Json)
        .map_err(relation_decision_error_response)
}

pub(crate) async fn reject_lesson_relation(
    State(state): State<AppState>,
    Path(relation_id): Path<i64>,
) -> Result<Json<LessonRelationDto>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let outcome = handle
        .reject_relation(relation_id, "operator")
        .await
        .map_err(|e| {
            tracing::warn!(relation_id, error = %e, "reject_lesson_relation: query failed");
            persist_error_response(e)
        })?;
    relation_decision_dto(outcome)
        .map(Json)
        .map_err(relation_decision_error_response)
}

pub(crate) async fn list_lesson_relations_for_lesson(
    State(state): State<AppState>,
    Path(lesson_id): Path<i64>,
) -> Result<Json<Vec<LessonRelationDto>>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let rows = handle
        .list_relations_for_lesson(lesson_id)
        .await
        .map_err(|e| {
            tracing::warn!(lesson_id, error = %e, "list_lesson_relations_for_lesson: query failed");
            persist_error_response(e)
        })?;
    Ok(Json(
        rows.into_iter().map(LessonRelationDto::from).collect(),
    ))
}
