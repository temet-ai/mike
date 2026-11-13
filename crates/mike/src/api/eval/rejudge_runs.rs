use crate::store::persistence::eval_repo::EvalPersist as _;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_rejudge::{RejudgeProgress, RejudgeRunRow};

#[derive(Debug, Serialize)]
pub(crate) struct RejudgeRunView {
    #[serde(flatten)]
    pub(crate) run: RejudgeRunRow,
    #[serde(flatten)]
    pub(crate) progress: RejudgeProgress,
    pub(crate) projected_finish_date: Option<DateTime<Utc>>,
}

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

fn projected_finish(run: &RejudgeRunRow, progress: &RejudgeProgress) -> Option<DateTime<Utc>> {
    if run.status == "done" || run.status == "cancelled" {
        return None;
    }
    Some(Utc::now() + Duration::days(progress.days_at_cap(run.daily_call_cap)))
}

async fn view(persist: &PersistHandle, run: RejudgeRunRow) -> Result<RejudgeRunView, Response> {
    let progress = persist
        .eval()
        .rejudge_progress(&run.window())
        .await
        .map_err(|e| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("persistence error: {e}"),
            )
        })?;
    Ok(RejudgeRunView {
        projected_finish_date: projected_finish(&run, &progress),
        run,
        progress,
    })
}

pub(crate) async fn list_rejudge_runs(State(state): State<AppState>) -> Response {
    let Some(persist) = state.persist() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled".to_owned(),
        );
    };
    let runs = match persist.eval().list_rejudge_runs().await {
        Ok(runs) => runs,
        Err(e) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("persistence error: {e}"),
            );
        }
    };
    let mut views = Vec::with_capacity(runs.len());
    for run in runs {
        match view(persist, run).await {
            Ok(v) => views.push(v),
            Err(resp) => return resp,
        }
    }
    Json(serde_json::json!({ "runs": views })).into_response()
}

pub(crate) async fn get_rejudge_run(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Response {
    let Some(persist) = state.persist() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "session persistence is disabled".to_owned(),
        );
    };
    match persist.eval().get_rejudge_run(id).await {
        Ok(Some(run)) => match view(persist, run).await {
            Ok(v) => Json(v).into_response(),
            Err(resp) => resp,
        },
        Ok(None) => error(
            StatusCode::NOT_FOUND,
            format!("no re-judge run with id {id}"),
        ),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}
