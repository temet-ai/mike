use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde_json::json;

use crate::state::AppState;

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled".to_owned(),
    )
}

pub(crate) async fn run_rollup(State(state): State<AppState>) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let today = Utc::now().date_naive();
    let rollup_id = match persist
        .current_or_claimed_action_effect_rollup_id(today)
        .await
    {
        Ok(id) => id,
        Err(e) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("persistence error: {e}"),
            );
        }
    };
    match persist
        .write_action_effect_rows(rollup_id, Utc::now())
        .await
    {
        Ok(rows_written) => {
            Json(json!({"rollup_id": rollup_id, "rows_written": rows_written})).into_response()
        }
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}
