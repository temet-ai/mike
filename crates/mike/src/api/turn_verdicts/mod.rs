mod dto;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::state::AppState;
use crate::store::persistence::turn_verdicts::{TurnVerdictFilter, VerdictMode};

use dto::{
    TurnVerdictListPage, TurnVerdictListQuery, TurnVerdictsPage, TurnVerdictsQuery, dtos, turn_dtos,
};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 500;

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled; turn verdicts are not available".to_owned(),
    )
}

fn persistence_error(e: &crate::store::persistence::PersistError) -> Response {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("persistence error: {e}"),
    )
}

fn parse_mode(raw: Option<&str>) -> Result<Option<VerdictMode>, Box<Response>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    VerdictMode::parse(raw).map(Some).ok_or_else(|| {
        Box::new(error(
            StatusCode::BAD_REQUEST,
            format!("mode must be one of live, shadow, eval; got {raw:?}"),
        ))
    })
}

fn parse_page(limit: Option<i64>, offset: Option<i64>) -> Result<(i64, i64), Box<Response>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(Box::new(error(
            StatusCode::BAD_REQUEST,
            format!("limit must be between 1 and {MAX_LIMIT}, got {limit}"),
        )));
    }
    let offset = offset.unwrap_or(0);
    if offset < 0 {
        return Err(Box::new(error(
            StatusCode::BAD_REQUEST,
            format!("offset must be zero or more, got {offset}"),
        )));
    }
    Ok((limit, offset))
}

pub(crate) async fn get_turn_verdicts(
    State(state): State<AppState>,
    Path(turn_id): Path<i64>,
    Query(query): Query<TurnVerdictsQuery>,
) -> Response {
    let (mode, (limit, offset)) = match (
        parse_mode(query.mode.as_deref()),
        parse_page(query.limit, query.offset),
    ) {
        (Ok(mode), Ok(page)) => (mode, page),
        (Err(resp), _) | (_, Err(resp)) => return *resp,
    };
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    match persist.operator_turn_exists(turn_id).await {
        Ok(true) => {}
        Ok(false) => {
            return error(
                StatusCode::NOT_FOUND,
                format!("no operator turn with id {turn_id}"),
            );
        }
        Err(e) => return persistence_error(&e),
    }
    let (live_verdict_id, legacy_label) = match tokio::try_join!(
        persist.turn_live_verdict_id(turn_id),
        persist.legacy_label_for_turn(turn_id)
    ) {
        Ok(pair) => pair,
        Err(e) => return persistence_error(&e),
    };
    match persist
        .list_turn_verdicts_for_turn(turn_id, mode, limit, offset)
        .await
    {
        Ok((rows, total)) => Json(TurnVerdictsPage {
            turn_id,
            live_verdict_id,
            legacy_label,
            verdicts: turn_dtos(rows, live_verdict_id),
            total,
            limit,
            offset,
        })
        .into_response(),
        Err(e) => persistence_error(&e),
    }
}

pub(crate) async fn list_turn_verdicts(
    State(state): State<AppState>,
    Query(query): Query<TurnVerdictListQuery>,
) -> Response {
    let (mode, (limit, offset)) = match (
        parse_mode(query.mode.as_deref()),
        parse_page(query.limit, query.offset),
    ) {
        (Ok(mode), Ok(page)) => (mode, page),
        (Err(resp), _) | (_, Err(resp)) => return *resp,
    };
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let filter = TurnVerdictFilter {
        mode,
        bundle_id: query.bundle_id,
        rubric_version: query.rubric_version,
        session_uuid: query.session_id,
        since: query.since,
        until: query.until,
        limit,
        offset,
    };
    match persist.list_turn_verdicts(&filter).await {
        Ok((rows, total)) => Json(TurnVerdictListPage {
            items: dtos(rows),
            total,
            limit,
            offset,
        })
        .into_response(),
        Err(e) => persistence_error(&e),
    }
}
