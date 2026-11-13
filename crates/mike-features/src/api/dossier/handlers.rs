use crate::store::persistence::dossier_repo::DossierPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde_json::json;

use super::envelope::{Paging, envelope, page};
use super::params::{DEFAULT_LIMIT, DossierParams, check_section, parse_as_of, parse_audience};
use crate::dossier::render::render_inputs;
use crate::dossier::view::full::{full_view, resolutions};
use crate::dossier::view::types::{DossierError, Needs};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled; the dossier is not available",
    )
}

fn dossier_error(e: &DossierError) -> Response {
    match e {
        DossierError::UnknownSession(_) | DossierError::UnknownTurn(_) => {
            error(StatusCode::NOT_FOUND, e.to_string())
        }
        DossierError::TurnNotClosedAt { .. } | DossierError::BudgetMismatch { .. } => {
            error(StatusCode::UNPROCESSABLE_ENTITY, e.to_string())
        }
        DossierError::Persist(_) => error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn session_db_id(persist: &PersistHandle, session_uuid: &str) -> Result<i64, Response> {
    match persist.resolve_session_db_id(session_uuid).await {
        Ok(Some(id)) => Ok(id),
        Ok(None) => Err(error(StatusCode::NOT_FOUND, "unknown session")),
        Err(e) => Err(error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

async fn turn_closed_at(
    persist: &PersistHandle,
    session_db_id: i64,
    turn_id: i64,
) -> Result<DateTime<Utc>, Response> {
    let located = persist
        .dossier()
        .locate_turn(turn_id)
        .await
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, format!("unknown turn {turn_id}")))?;
    if located.session_db_id != session_db_id {
        return Err(error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("turn {turn_id} belongs to another session"),
        ));
    }
    located.closed_at.ok_or_else(|| {
        error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("turn {turn_id} is still open"),
        )
    })
}

async fn resolve_at(
    persist: &PersistHandle,
    session_db_id: i64,
    params: &DossierParams,
) -> Result<DateTime<Utc>, Response> {
    if let Some(turn_id) = params.as_of_turn {
        return turn_closed_at(persist, session_db_id, turn_id).await;
    }
    if let Some(at) = params.as_of {
        return Ok(at);
    }
    if params.audience.as_deref() == Some("judge")
        && let Some(turn_id) = params.turn_id
    {
        return turn_closed_at(persist, session_db_id, turn_id).await;
    }
    Ok(Utc::now())
}

fn paging(params: &DossierParams) -> Paging {
    Paging {
        offset: params.offset.unwrap_or(0),
        limit: params.limit.unwrap_or(DEFAULT_LIMIT).max(1),
    }
}

pub async fn get_session_dossier(
    State(state): State<AppState>,
    Path(session_uuid): Path<String>,
    Query(params): Query<DossierParams>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let session = match session_db_id(persist, &session_uuid).await {
        Ok(id) => id,
        Err(response) => return response,
    };
    if let Err(message) = check_section(params.section.as_deref()) {
        return error(StatusCode::UNPROCESSABLE_ENTITY, message);
    }
    let at = match resolve_at(persist, session, &params).await {
        Ok(at) => at,
        Err(response) => return response,
    };
    let (as_of, audience) = match parse_as_of(at, params.mode.as_deref())
        .and_then(|as_of| parse_audience(&params).map(|a| (as_of, a)))
    {
        Ok(parsed) => parsed,
        Err(message) => return error(StatusCode::UNPROCESSABLE_ENTITY, message),
    };
    let needs = Needs::for_inspection(audience.as_ref().map(|(audience, _)| audience));
    let inputs = match crate::dossier::view::load(persist, session, as_of, needs).await {
        Ok(inputs) => inputs,
        Err(e) => return dossier_error(&e),
    };
    let rendered = match &audience {
        Some((audience, budget)) => match render_inputs(&inputs, audience, *budget) {
            Ok(r) => Some(r),
            Err(e) => return dossier_error(&e),
        },
        None => None,
    };
    let view = full_view(&inputs);
    Json(envelope(
        &inputs,
        &view,
        rendered.as_ref(),
        paging(&params),
        params.section.as_deref(),
    ))
    .into_response()
}

pub async fn get_session_prompt_stream(
    State(state): State<AppState>,
    Path(session_uuid): Path<String>,
    Query(params): Query<DossierParams>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let session = match session_db_id(persist, &session_uuid).await {
        Ok(id) => id,
        Err(response) => return response,
    };
    let as_of = match parse_as_of(
        params.as_of.unwrap_or_else(Utc::now),
        params.mode.as_deref(),
    ) {
        Ok(as_of) => as_of,
        Err(message) => return error(StatusCode::UNPROCESSABLE_ENTITY, message),
    };
    let needs = Needs {
        stream: true,
        ..Needs::default()
    };
    match crate::dossier::view::load(persist, session, as_of, needs).await {
        Ok(inputs) => {
            let view = full_view(&inputs);
            Json(json!({
                "session_uuid": inputs.session.session_uuid,
                "as_of": as_of.at,
                "mode": as_of.mode,
                "stream": page(&view.stream, paging(&params)),
            }))
            .into_response()
        }
        Err(e) => dossier_error(&e),
    }
}

pub async fn get_turn_facts(State(state): State<AppState>, Path(turn_id): Path<i64>) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let located = match persist.dossier().locate_turn(turn_id).await {
        Ok(Some(located)) => located,
        Ok(None) => return error(StatusCode::NOT_FOUND, format!("unknown turn {turn_id}")),
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let facts = match persist.dossier().fetch_turn_facts_by_id(turn_id).await {
        Ok(facts) => facts,
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let needs = Needs {
        stream: true,
        ..Needs::default()
    };
    let inputs = match crate::dossier::view::load(
        persist,
        located.session_db_id,
        crate::dossier::view::AsOf::knowledge(Utc::now()),
        needs,
    )
    .await
    {
        Ok(inputs) => inputs,
        Err(e) => return dossier_error(&e),
    };
    let now = resolutions(&inputs).remove(&turn_id);
    let next_request = facts.as_ref().and_then(|f| f.next_genuine_request_id);
    let next_text = next_request.and_then(|id| {
        inputs
            .stream
            .iter()
            .find(|r| r.request_id == id)
            .map(|r| r.text.clone())
    });
    Json(json!({
        "turn_id": turn_id,
        "session_uuid": located.session_uuid,
        "opened_by": located.opened_by,
        "facts": facts,
        "next_genuine": facts.as_ref().map(|f| json!({
            "request_id": f.next_genuine_request_id,
            "turn_id": f.next_genuine_turn_id,
            "at": f.next_genuine_at,
            "text": next_text,
        })),
        "resolution_now": now.map(|r| json!({
            "effective_kind": r.effective.kind.j2_code(),
            "next_genuine": r.next_genuine.map(|n| json!({"request_id": n.request_id, "turn_id": n.turn_id, "at": n.at})),
            "machine_segment_ids": r.machine_segment_ids,
        })),
    }))
    .into_response()
}
