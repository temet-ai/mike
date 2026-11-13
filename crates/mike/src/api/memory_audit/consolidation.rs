use crate::store::persistence::lessons_repo::LessonsPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};

use super::{Paged, clamp_limit, clamp_offset, parse_session_filter};
use crate::api::handlers::lesson_error;
use crate::state::AppState;
use crate::store::persistence::consolidation_reads::{
    ConsolidationActionRow, ConsolidationDecisionsFilter, ConsolidationRunRow,
    ConsolidationRunsFilter, LessonHistory, LessonInjectionRow, RunnerUp,
};
use crate::web_base::{persist_error_response, persist_unavailable_response};

#[derive(Debug, Serialize)]
pub(crate) struct ConsolidationRunDto {
    pub(crate) id: i64,
    pub(crate) uuid: String,
    pub(crate) kind: String,
    pub(crate) trigger: String,
    pub(crate) status: String,
    pub(crate) started_at: String,
    pub(crate) finished_at: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) action_count: i64,
}

impl From<ConsolidationRunRow> for ConsolidationRunDto {
    fn from(r: ConsolidationRunRow) -> Self {
        Self {
            id: r.id,
            uuid: r.uuid,
            kind: r.kind,
            trigger: r.trigger,
            status: r.status,
            started_at: r.started_at.to_rfc3339(),
            finished_at: r.finished_at.map(|t| t.to_rfc3339()),
            error: r.error,
            action_count: r.action_count,
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct ConsolidationActionDto {
    pub(crate) id: i64,
    pub(crate) run_id: i64,
    pub(crate) run_kind: String,
    pub(crate) lesson_id: Option<i64>,
    pub(crate) lesson_uuid: String,
    pub(crate) project_id: Option<i64>,
    pub(crate) project_path: Option<String>,
    pub(crate) session_id: Option<i64>,
    pub(crate) session_uuid: Option<String>,
    pub(crate) source_turn_id: Option<i64>,
    pub(crate) files_touched: Option<Vec<String>>,
    pub(crate) agent_id: Option<i64>,
    pub(crate) lesson_text: String,
    pub(crate) action_kind: String,
    pub(crate) outcome: String,
    pub(crate) error: Option<String>,
    pub(crate) before_stage: Option<String>,
    pub(crate) after_stage: Option<String>,
    pub(crate) decided_by: Option<String>,
    pub(crate) distance: Option<f32>,
    pub(crate) threshold: Option<f32>,
    pub(crate) embedding_model: Option<String>,
    pub(crate) counterpart_lesson_id: Option<i64>,
    pub(crate) counterpart_uuid: Option<String>,
    pub(crate) counterpart_text: Option<String>,
    pub(crate) runners_up: Vec<RunnerUp>,
    pub(crate) rank_detail: Option<serde_json::Value>,
    pub(crate) injection_count: Option<i32>,
    pub(crate) created_at: String,
}

impl From<ConsolidationActionRow> for ConsolidationActionDto {
    fn from(a: ConsolidationActionRow) -> Self {
        Self {
            id: a.id,
            run_id: a.run_id,
            run_kind: a.run_kind,
            lesson_id: a.lesson_id,
            lesson_uuid: a.lesson_uuid,
            project_id: a.project_id,
            project_path: a.project_path,
            session_id: a.session_id,
            session_uuid: a.session_uuid,
            source_turn_id: a.source_turn_id,
            files_touched: a.files_touched,
            agent_id: a.agent_id,
            lesson_text: a.lesson_text,
            action_kind: a.action_kind,
            outcome: a.outcome,
            error: a.error,
            before_stage: a.before_stage,
            after_stage: a.after_stage,
            decided_by: a.decided_by,
            distance: a.distance,
            threshold: a.threshold,
            embedding_model: a.embedding_model,
            counterpart_lesson_id: a.counterpart_lesson_id,
            counterpart_uuid: a.counterpart_uuid,
            counterpart_text: a.counterpart_text,
            runners_up: a.runners_up.0,
            rank_detail: a.rank_detail.map(|j| j.0),
            injection_count: a.injection_count,
            created_at: a.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct ConsolidationRunDetailDto {
    pub(crate) run: ConsolidationRunDto,
    pub(crate) actions: Vec<ConsolidationActionDto>,
}

#[derive(Debug, Serialize)]
pub(crate) struct LessonInjectionDto {
    pub(crate) id: i64,
    pub(crate) lesson_id: i64,
    pub(crate) request_id: i64,
    pub(crate) session_id: Option<i64>,
    pub(crate) session_uuid: Option<String>,
    pub(crate) injected_at: String,
    pub(crate) lane: String,
    pub(crate) persona: Option<String>,
}

impl From<LessonInjectionRow> for LessonInjectionDto {
    fn from(i: LessonInjectionRow) -> Self {
        Self {
            id: i.id,
            lesson_id: i.lesson_id,
            request_id: i.request_id,
            session_id: i.session_id,
            session_uuid: i.session_uuid,
            injected_at: i.injected_at.to_rfc3339(),
            lane: i.lane,
            persona: i.persona,
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct LessonHistoryDto {
    pub(crate) lesson_id: i64,
    pub(crate) lesson_uuid: String,
    pub(crate) lesson_text: String,
    pub(crate) kind: String,
    pub(crate) stage: String,
    pub(crate) created_at: String,
    pub(crate) session_uuid: Option<String>,
    pub(crate) source_turn_id: Option<i64>,
    pub(crate) actions: Vec<ConsolidationActionDto>,
    pub(crate) injections: Vec<LessonInjectionDto>,
}

impl From<LessonHistory> for LessonHistoryDto {
    fn from(h: LessonHistory) -> Self {
        Self {
            lesson_id: h.header.lesson_id,
            lesson_uuid: h.header.lesson_uuid,
            lesson_text: h.header.lesson_text,
            kind: h.header.kind,
            stage: h.header.stage,
            created_at: h.header.created_at.to_rfc3339(),
            session_uuid: h.header.session_uuid,
            source_turn_id: h.header.source_turn_id,
            actions: h
                .actions
                .into_iter()
                .map(ConsolidationActionDto::from)
                .collect(),
            injections: h
                .injections
                .into_iter()
                .map(LessonInjectionDto::from)
                .collect(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ConsolidationRunsListParams {
    #[serde(default)]
    pub(crate) kind: Option<String>,
    #[serde(default)]
    pub(crate) status: Option<String>,
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) session: Option<String>,
    #[serde(default)]
    pub(crate) file: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<i64>,
    #[serde(default)]
    pub(crate) offset: Option<i64>,
}

pub(crate) async fn list_consolidation_runs(
    State(state): State<AppState>,
    Query(params): Query<ConsolidationRunsListParams>,
) -> Result<Json<Paged<ConsolidationRunDto>>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let session = parse_session_filter(params.session.as_deref())
        .map_err(|msg| lesson_error(StatusCode::BAD_REQUEST, msg))?;
    let filter = ConsolidationRunsFilter {
        kind: params.kind,
        status: params.status,
        project: params.project,
        session,
        file: params.file,
        limit: clamp_limit(params.limit),
        offset: clamp_offset(params.offset),
    };
    let (total_count, rows) = handle
        .lessons()
        .list_consolidation_runs(&filter)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "list_consolidation_runs: query failed");
            persist_error_response(e)
        })?;
    Ok(Json(Paged {
        items: rows.into_iter().map(ConsolidationRunDto::from).collect(),
        total_count,
        limit: filter.limit,
        offset: filter.offset,
    }))
}

pub(crate) async fn get_consolidation_run(
    State(state): State<AppState>,
    Path(run_id): Path<i64>,
) -> Result<Json<ConsolidationRunDetailDto>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let found = handle
        .lessons()
        .get_consolidation_run(run_id)
        .await
        .map_err(|e| {
            tracing::warn!(run_id, error = %e, "get_consolidation_run: query failed");
            persist_error_response(e)
        })?;
    let Some((run, actions)) = found else {
        return Err(lesson_error(
            StatusCode::NOT_FOUND,
            "consolidation run not found",
        ));
    };
    Ok(Json(ConsolidationRunDetailDto {
        run: run.into(),
        actions: actions
            .into_iter()
            .map(ConsolidationActionDto::from)
            .collect(),
    }))
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ConsolidationDecisionsListParams {
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) session: Option<String>,
    #[serde(default)]
    pub(crate) file: Option<String>,
    #[serde(default)]
    pub(crate) action_kind: Option<String>,
    #[serde(default)]
    pub(crate) limit: Option<i64>,
    #[serde(default)]
    pub(crate) offset: Option<i64>,
}

pub(crate) async fn list_consolidation_decisions(
    State(state): State<AppState>,
    Query(params): Query<ConsolidationDecisionsListParams>,
) -> Result<Json<Paged<ConsolidationActionDto>>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let session = parse_session_filter(params.session.as_deref())
        .map_err(|msg| lesson_error(StatusCode::BAD_REQUEST, msg))?;
    let filter = ConsolidationDecisionsFilter {
        project: params.project,
        session,
        file: params.file,
        action_kind: params.action_kind,
        limit: clamp_limit(params.limit),
        offset: clamp_offset(params.offset),
    };
    let (total_count, rows) = handle
        .lessons()
        .list_consolidation_decisions(&filter)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "list_consolidation_decisions: query failed");
            persist_error_response(e)
        })?;
    Ok(Json(Paged {
        items: rows.into_iter().map(ConsolidationActionDto::from).collect(),
        total_count,
        limit: filter.limit,
        offset: filter.offset,
    }))
}

pub(crate) async fn get_lesson_history(
    State(state): State<AppState>,
    Path(lesson_id): Path<i64>,
) -> Result<Json<LessonHistoryDto>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let history = handle
        .lessons()
        .get_lesson_history(lesson_id)
        .await
        .map_err(|e| {
            tracing::warn!(lesson_id, error = %e, "get_lesson_history: query failed");
            persist_error_response(e)
        })?;
    let Some(history) = history else {
        return Err(lesson_error(StatusCode::NOT_FOUND, "lesson not found"));
    };
    Ok(Json(history.into()))
}

pub(crate) async fn list_session_memory_actions(
    State(state): State<AppState>,
    Path(session_uuid): Path<String>,
) -> Result<Json<Vec<ConsolidationActionDto>>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let Ok(uuid) = uuid::Uuid::parse_str(&session_uuid) else {
        return Err(lesson_error(StatusCode::NOT_FOUND, "unknown session"));
    };
    let rows = handle
        .lessons().list_session_memory_actions(uuid)
        .await
        .map_err(|e| {
            tracing::warn!(session_id = %session_uuid, error = %e, "list_session_memory_actions: query failed");
            persist_error_response(e)
        })?;
    let Some(rows) = rows else {
        return Err(lesson_error(StatusCode::NOT_FOUND, "unknown session"));
    };
    Ok(Json(
        rows.into_iter().map(ConsolidationActionDto::from).collect(),
    ))
}
