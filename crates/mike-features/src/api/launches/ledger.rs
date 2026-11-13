use crate::store::persistence::launch_repo::LaunchPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use uuid::Uuid;

use crate::launch::cli::ledger_wire::{
    ChannelPresent, ClosedBody, LaunchId, LedgerHello, LinkBody, LinkReply, RunQuery, TurnQuery,
    TurnSeen, launch_settings,
};
use crate::state::AppState;
use crate::store::persistence::PersistError;
use crate::store::persistence::launch::{LaunchOutcome, LaunchSurfaceRefs, NewLaunch, NewRun};

fn unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({
            "error": "ledger_unavailable",
            "message": "this proxy has no database, so it cannot record launches",
        })),
    )
        .into_response()
}

fn failed(err: &PersistError) -> Response {
    tracing::warn!(error = %err, "launch ledger write through the proxy failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "ledger_failed", "message": err.to_string()})),
    )
        .into_response()
}

fn done(result: Result<(), PersistError>) -> Response {
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => failed(&e),
    }
}

pub async fn hello(State(state): State<AppState>) -> Json<LedgerHello> {
    let launch_brief = state.feature_toggle_overlay.load().launch_brief;
    let (database, settings) = match state.persist() {
        Some(handle) => (
            true,
            handle
                .fetch_settings()
                .await
                .map(launch_settings)
                .unwrap_or_default(),
        ),
        None => (false, Vec::new()),
    };
    Json(LedgerHello {
        database,
        launch_brief,
        settings,
    })
}

pub async fn upsert_run(State(state): State<AppState>, Json(run): Json<NewRun>) -> Response {
    match state.persist() {
        Some(h) => done(h.launch().upsert_run(&run).await),
        None => unavailable(),
    }
}

pub async fn find_run(
    State(state): State<AppState>,
    Query(query): Query<RunQuery>,
) -> Response {
    let Some(h) = state.persist() else {
        return unavailable();
    };
    match h.launch().find_run(&query.run).await {
        Ok(Some(row)) => Json(row).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => failed(&e),
    }
}

pub async fn insert_launch(
    State(state): State<AppState>,
    Json(new): Json<NewLaunch>,
) -> Response {
    let Some(h) = state.persist() else {
        return unavailable();
    };
    match h.launch().insert_launch(&new).await {
        Ok(id) => (StatusCode::CREATED, Json(LaunchId { id })).into_response(),
        Err(e) => failed(&e),
    }
}

pub async fn record_opened(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(refs): Json<LaunchSurfaceRefs>,
) -> Response {
    match state.persist() {
        Some(h) => done(h.launch().record_launch_opened(id, &refs).await),
        None => unavailable(),
    }
}

pub async fn record_outcome(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(outcome): Json<LaunchOutcome>,
) -> Response {
    match state.persist() {
        Some(h) => done(h.launch().record_launch_outcome(id, &outcome).await),
        None => unavailable(),
    }
}

pub async fn record_closed(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ClosedBody>,
) -> Response {
    match state.persist() {
        Some(h) => done(
            h.launch()
                .record_launch_closed(id, &body.reason, body.process_exited)
                .await,
        ),
        None => unavailable(),
    }
}

pub async fn channel_present(
    State(state): State<AppState>,
    Path(session): Path<Uuid>,
) -> Response {
    let Some(h) = state.persist() else {
        return unavailable();
    };
    match h.launch().session_has_remote_control_channel(session).await {
        Ok(present) => Json(ChannelPresent { present }).into_response(),
        Err(e) => failed(&e),
    }
}

pub async fn link_channel(
    State(state): State<AppState>,
    Path(session): Path<Uuid>,
    Json(body): Json<LinkBody>,
) -> Response {
    let Some(h) = state.persist() else {
        return unavailable();
    };
    match h
        .launch()
        .link_channel_to_session(&body.cse_id, session, &body.cwd)
        .await
    {
        Ok(outcome) => Json(LinkReply { outcome }).into_response(),
        Err(e) => failed(&e),
    }
}

pub async fn prompt_turn_seen(
    State(state): State<AppState>,
    Path(session): Path<Uuid>,
    Query(query): Query<TurnQuery>,
) -> Response {
    let Some(h) = state.persist() else {
        return unavailable();
    };
    match h
        .launch()
        .session_prompt_turn_seen(session, query.since, &query.prefix)
        .await
    {
        Ok(seen) => Json(TurnSeen { seen }).into_response(),
        Err(e) => failed(&e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_launch_settings_leave_the_proxy() {
        let rows = vec![
            ("launch_backend".to_string(), json!("herdr")),
            ("steerer_openrouter_key".to_string(), json!("secret")),
        ];
        assert_eq!(
            launch_settings(rows),
            vec![("launch_backend".to_string(), json!("herdr"))]
        );
    }
}
