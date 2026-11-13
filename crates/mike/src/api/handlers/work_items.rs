use crate::store::persistence::launch_repo::LaunchPersist as _;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::state::AppState;
use crate::work_items::ledger::Ledger;

async fn session_db_id(state: &AppState, id: &str, op: &'static str) -> Result<i64, StatusCode> {
    if uuid::Uuid::parse_str(id).is_err() {
        return Err(StatusCode::NOT_FOUND);
    }
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    handle
        .resolve_session_db_id(id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, session_id = %id, op, "resolve session failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)
}

pub(crate) async fn get_session_ledger(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Ledger>, StatusCode> {
    let db_id = session_db_id(&state, &id, "get_session_ledger").await?;
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let ledger = handle
        .launch()
        .work_item_ledger(db_id, &id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, session_id = %id, "get_session_ledger: query failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(ledger))
}

pub(crate) async fn post_session_work_items_backfill(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<BackfillStarted>), StatusCode> {
    let db_id = session_db_id(&state, &id, "post_session_work_items_backfill").await?;
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let handle = handle.clone();
    let session_id = id.clone();
    tokio::spawn(async move {
        match handle.launch().backfill_work_items(db_id).await {
            Ok(counts) => tracing::info!(
                session_id = %session_id,
                launches = counts.launches,
                notifications = counts.notifications,
                "work item backfill finished"
            ),
            Err(e) => {
                tracing::warn!(session_id = %session_id, error = %e, "work item backfill failed")
            }
        }
    });
    Ok((
        StatusCode::ACCEPTED,
        Json(BackfillStarted {
            session_id: id,
            status: "started",
        }),
    ))
}

#[derive(serde::Serialize)]
pub(crate) struct BackfillStarted {
    session_id: String,
    status: &'static str,
}
