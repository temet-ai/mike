pub(crate) mod patterns;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::dossier::view::types::{AsOf, AsOfMode};
use crate::semantic::egress::{egress_state, url_host};
use crate::semantic::neighbours::Retriever;
use crate::semantic::retrieve::{
    Audience, QuerySubject, RetrievedKind, Scope, SemanticQuery, default_kinds,
};
use crate::semantic::{SemanticError, config::RetrievalScopeSetting};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;

pub(crate) fn error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

pub(crate) fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled".to_owned(),
    )
}

pub(crate) fn persist_error(e: &crate::store::persistence::PersistError) -> Response {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("persistence error: {e}"),
    )
}

async fn floors_view(
    persist: &PersistHandle,
    key: &str,
    quantile: f64,
) -> Result<serde_json::Value, Response> {
    let in_force = persist
        .semantic_floors_in_force(key, quantile)
        .await
        .map_err(|e| persist_error(&e))?;
    let monitoring = persist
        .semantic_floors_latest_monitoring(key)
        .await
        .map_err(|e| persist_error(&e))?;
    Ok(json!({ "in_force": in_force, "latest_monitoring": monitoring }))
}

async fn status_body(
    state: &AppState,
    persist: &PersistHandle,
) -> Result<serde_json::Value, Response> {
    let handle = state.semantic().load();
    let snapshot = state.embedding_service().current_config();
    let embedders = persist
        .list_semantic_embedders()
        .await
        .map_err(|e| persist_error(&e))?;
    let current = embedders.iter().find(|e| e.status == "current").cloned();
    let counts = persist
        .semantic_item_counts()
        .await
        .map_err(|e| persist_error(&e))?;
    let (floors, backlog) = match &current {
        Some(c) => (
            floors_view(persist, &c.embedder_key, handle.config.floor_quantile).await?,
            persist
                .semantic_backlog(&c.embedder_key)
                .await
                .map_err(|e| persist_error(&e))?,
        ),
        None => (json!({ "in_force": [], "latest_monitoring": [] }), 0),
    };
    Ok(json!({
        "toggle": state.feature_toggle_overlay.load().semantic_context,
        "egress": {
            "state": egress_state(&snapshot, handle.config.allow_remote_embedding),
            "host": url_host(&snapshot.url),
        },
        "current_embedder": current,
        "embedders": embedders,
        "floors": floors,
        "item_counts": counts,
        "backlog_turns": backlog,
        "worker": handle.status,
        "config": handle.config,
    }))
}

pub(crate) async fn get_semantic_status(State(state): State<AppState>) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    match status_body(&state, persist).await {
        Ok(body) => Json(body).into_response(),
        Err(resp) => resp,
    }
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct NeighboursQuery {
    pub(crate) audience: Option<String>,
    pub(crate) kinds: Option<String>,
    pub(crate) scope: Option<String>,
    pub(crate) as_of: Option<DateTime<Utc>>,
    pub(crate) as_of_mode: Option<String>,
    pub(crate) k: Option<usize>,
    pub(crate) offset: Option<usize>,
}

fn parse_kinds(raw: Option<&str>, audience: Audience) -> Result<Vec<RetrievedKind>, Box<Response>> {
    let Some(raw) = raw.filter(|r| !r.is_empty()) else {
        return Ok(default_kinds(audience));
    };
    raw.split(',')
        .map(|k| {
            RetrievedKind::parse(k.trim()).ok_or_else(|| {
                Box::new(error(
                    StatusCode::BAD_REQUEST,
                    format!("unknown kind {k:?}"),
                ))
            })
        })
        .collect()
}

fn default_scope(setting: RetrievalScopeSetting) -> Scope {
    match setting {
        RetrievalScopeSetting::Operator => Scope::Operator,
        RetrievalScopeSetting::Project => Scope::Project,
        RetrievalScopeSetting::Session => Scope::Session,
    }
}

fn build_query(
    turn_id: i64,
    q: &NeighboursQuery,
    closed_at: Option<DateTime<Utc>>,
    state: &AppState,
) -> Result<SemanticQuery, Box<Response>> {
    let cfg = state.semantic().config();
    let audience = match q.audience.as_deref() {
        None => Audience::Steerer,
        Some(raw) => Audience::parse(raw).ok_or_else(|| {
            Box::new(error(
                StatusCode::BAD_REQUEST,
                format!("unknown audience {raw:?}"),
            ))
        })?,
    };
    let scope = match q.scope.as_deref() {
        None => default_scope(cfg.retrieval_scope),
        Some(raw) => Scope::parse(raw).ok_or_else(|| {
            Box::new(error(
                StatusCode::BAD_REQUEST,
                format!("unknown scope {raw:?}"),
            ))
        })?,
    };
    let mode = match q.as_of_mode.as_deref() {
        None | Some("knowledge") => AsOfMode::Knowledge,
        Some("reconstructed") => AsOfMode::Reconstructed,
        Some(other) => {
            return Err(Box::new(error(
                StatusCode::BAD_REQUEST,
                format!("unknown as_of_mode {other:?}"),
            )));
        }
    };
    Ok(SemanticQuery {
        subject: QuerySubject::ClosedTurn(turn_id),
        audience,
        kinds: parse_kinds(q.kinds.as_deref(), audience)?,
        scope,
        as_of: AsOf {
            at: q.as_of.or(closed_at).unwrap_or_else(Utc::now),
            mode,
        },
        k: q.k.unwrap_or(cfg.retrieval_k).clamp(1, 100),
        offset: q.offset.unwrap_or(0),
    })
}

pub(crate) async fn get_semantic_neighbours(
    State(state): State<AppState>,
    Path(turn_id): Path<i64>,
    Query(q): Query<NeighboursQuery>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let closed_at = match persist.locate_turn(turn_id).await {
        Ok(Some(t)) => t.closed_at,
        Ok(None) => {
            return error(
                StatusCode::NOT_FOUND,
                format!("no operator turn with id {turn_id}"),
            );
        }
        Err(e) => return persist_error(&e),
    };
    let query = match build_query(turn_id, &q, closed_at, &state) {
        Ok(query) => query,
        Err(resp) => return *resp,
    };
    if !state.feature_toggle_overlay.load().semantic_context {
        return Json(json!({ "status": "disabled" })).into_response();
    }
    let config = state.semantic().config();
    let embedding = state.embedding_service();
    let retriever = Retriever {
        persist,
        embedding: Some(&embedding),
        config: &config,
        live_consumer: false,
    };
    match retriever.neighbours(&query).await {
        Ok(set) => Json(set).into_response(),
        Err(SemanticError::Query(e)) => error(StatusCode::BAD_REQUEST, e.to_string()),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

pub(crate) async fn get_semantic_item(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    match persist.get_semantic_item(id).await {
        Ok(Some(item)) => Json(item).into_response(),
        Ok(None) => error(
            StatusCode::NOT_FOUND,
            format!("no semantic item with id {id}"),
        ),
        Err(e) => persist_error(&e),
    }
}
