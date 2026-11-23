use super::knowledge_consolidation_dto::{decisions_json, statements_json};
use crate::knowledge_consolidation::settle::settle_conflict;
use crate::knowledge_consolidation::undo::{DecisionError, apply_decision, undo_decision};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::consolidation::{DecisionCursor, DecisionFilter, DecisionRow};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;

const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 500;

#[derive(Debug, Default, Deserialize)]
pub struct SubjectsQuery {
    project: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct DecisionsQuery {
    project: Option<String>,
    subject: Option<i64>,
    before: Option<i64>,
    limit: Option<i64>,
    verdict: Option<String>,
    open: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct MeasuresQuery {
    project: Option<String>,
    since: Option<DateTime<Utc>>,
    limit: Option<i64>,
}

fn refuse(status: StatusCode, message: impl std::fmt::Display) -> Response {
    (status, Json(json!({ "error": message.to_string() }))).into_response()
}

fn persist(state: &AppState) -> Result<&PersistHandle, Response> {
    state
        .persist()
        .ok_or_else(|| refuse(StatusCode::SERVICE_UNAVAILABLE, "persistence is disabled"))
}

async fn project_id(persist: &PersistHandle, selector: Option<&str>) -> Result<i64, Response> {
    let Some(selector) = selector.map(str::trim).filter(|s| !s.is_empty()) else {
        return Err(refuse(
            StatusCode::BAD_REQUEST,
            "query parameter `project` is required: a project id, uuid, path or name",
        ));
    };
    match persist.knowledge().find_knowledge_project(selector).await {
        Ok(Some(project)) => Ok(project.id),
        Ok(None) => Err(refuse(
            StatusCode::NOT_FOUND,
            format!("no project matches `{selector}`"),
        )),
        Err(error) => Err(refuse(StatusCode::BAD_REQUEST, error)),
    }
}

fn limit(requested: Option<i64>) -> i64 {
    requested.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

fn failed(error: impl std::fmt::Display) -> Response {
    refuse(StatusCode::INTERNAL_SERVER_ERROR, error)
}

pub async fn list_subjects(
    State(state): State<AppState>,
    Query(query): Query<SubjectsQuery>,
) -> Response {
    let persist = match persist(&state) {
        Ok(persist) => persist,
        Err(response) => return response,
    };
    let project = match project_id(persist, query.project.as_deref()).await {
        Ok(id) => id,
        Err(response) => return response,
    };
    match persist
        .knowledge()
        .list_knowledge_subjects(project, limit(query.limit), query.offset.unwrap_or(0).max(0))
        .await
    {
        Ok(subjects) => Json(json!({ "project_id": project, "subjects": subjects })).into_response(),
        Err(error) => failed(error),
    }
}

pub async fn list_decisions(
    State(state): State<AppState>,
    Query(query): Query<DecisionsQuery>,
) -> Response {
    let persist = match persist(&state) {
        Ok(persist) => persist,
        Err(response) => return response,
    };
    let project = match project_id(persist, query.project.as_deref()).await {
        Ok(id) => id,
        Err(response) => return response,
    };
    let cursor = match query.before {
        None => None,
        Some(id) => match persist.knowledge().get_consolidation_decision(id).await {
            Ok(Some(row)) => Some(DecisionCursor {
                created_at: row.created_at,
                id: row.id,
            }),
            Ok(None) => {
                return refuse(
                    StatusCode::BAD_REQUEST,
                    format!("`before` names decision {id}, which does not exist"),
                );
            }
            Err(error) => return failed(error),
        },
    };
    let limit = limit(query.limit);
    let open_only = query.open.unwrap_or(false);
    let mut verdicts: Option<Vec<String>> = query.verdict.as_deref().map(|list| {
        list.split(',')
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
            .collect()
    });
    if open_only && verdicts.is_none() {
        verdicts = Some(vec!["conflict_open".to_owned()]);
    }
    let filter = DecisionFilter {
        verdicts: verdicts.as_deref(),
        open_only,
    };
    match persist
        .knowledge()
        .list_consolidation_decisions_filtered(project, query.subject, cursor, limit, filter)
        .await
    {
        Ok(rows) => {
            let decisions = match decisions_json(persist, &rows).await {
                Ok(decisions) => decisions,
                Err(error) => return failed(error),
            };
            let next_before = (rows.len() as i64 == limit)
                .then(|| rows.last().map(|d| d.id))
                .flatten();
            Json(json!({ "project_id": project, "decisions": decisions, "next_before": next_before }))
                .into_response()
        }
        Err(error) => failed(error),
    }
}

pub async fn list_measures(
    State(state): State<AppState>,
    Query(query): Query<MeasuresQuery>,
) -> Response {
    let persist = match persist(&state) {
        Ok(persist) => persist,
        Err(response) => return response,
    };
    let project = match project_id(persist, query.project.as_deref()).await {
        Ok(id) => id,
        Err(response) => return response,
    };
    match persist
        .knowledge()
        .list_consolidation_measures(Some(project), query.since, limit(query.limit))
        .await
    {
        Ok(measures) => Json(json!({ "project_id": project, "measures": measures })).into_response(),
        Err(error) => failed(error),
    }
}

async fn answer(
    persist: &PersistHandle,
    result: Result<DecisionRow, DecisionError>,
) -> Response {
    match result {
        Ok(decision) => match decisions_json(persist, &[decision]).await {
            Ok(mut rows) => Json(json!({ "decision": rows.pop() })).into_response(),
            Err(error) => failed(error),
        },
        Err(error) => decision_error(error),
    }
}

fn decision_error(error: DecisionError) -> Response {
    match error {
        DecisionError::NotFound => refuse(StatusCode::NOT_FOUND, "no such decision"),
        DecisionError::Conflict(message) => refuse(StatusCode::CONFLICT, message),
        DecisionError::Failed(message) => failed(message),
    }
}

pub async fn undo(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    match persist(&state) {
        Ok(persist) => answer(persist, undo_decision(persist, id).await).await,
        Err(response) => response,
    }
}

async fn this_machine() -> Option<uuid::Uuid> {
    crate::machine_identity::load_default()
        .await
        .ok()
        .map(|identity| identity.machine_uuid)
}

pub async fn apply(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    let persist = match persist(&state) {
        Ok(persist) => persist,
        Err(response) => return response,
    };
    let machine = this_machine().await;
    answer(persist, apply_decision(persist, machine, id).await).await
}

#[derive(Debug, Deserialize)]
pub struct SettleBody {
    keep_statement_id: i64,
}

pub async fn settle(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<SettleBody>,
) -> Response {
    let persist = match persist(&state) {
        Ok(persist) => persist,
        Err(response) => return response,
    };
    let machine = this_machine().await;
    match settle_conflict(persist, machine, id, body.keep_statement_id).await {
        Ok(settled) => match decisions_json(persist, &[settled.decision, settled.conflict]).await {
            Ok(mut rows) => {
                let conflict = rows.pop();
                let decision = rows.pop();
                Json(json!({ "decision": decision, "conflict": conflict })).into_response()
            }
            Err(error) => failed(error),
        },
        Err(error) => decision_error(error),
    }
}

pub async fn get_subject(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    let persist = match persist(&state) {
        Ok(persist) => persist,
        Err(response) => return response,
    };
    let knowledge = persist.knowledge();
    let summary = match knowledge.get_knowledge_subject(id).await {
        Ok(Some(summary)) => summary,
        Ok(None) => return refuse(StatusCode::NOT_FOUND, format!("no subject {id}")),
        Err(error) => return failed(error),
    };
    let ids = match knowledge.subject_statement_ids_all(id).await {
        Ok(ids) => ids,
        Err(error) => return failed(error),
    };
    let statements = match statements_json(persist, &ids).await {
        Ok(statements) => statements,
        Err(error) => return failed(error),
    };
    let mut value = match serde_json::to_value(&summary) {
        Ok(value) => value,
        Err(error) => return failed(error),
    };
    if let serde_json::Value::Object(map) = &mut value {
        map.insert("statements".to_owned(), json!(statements));
    }
    Json(value).into_response()
}
