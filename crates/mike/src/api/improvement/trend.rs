use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::state::AppState;
use crate::store::persistence::change_log::{ChangeLogFilter, ChangeLogRow};
use crate::store::persistence::improvement::trend::WeeklyShare;

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled".to_owned(),
    )
}

const MARKER_KINDS: [&str; 2] = ["lesson_decision", "directive_decision"];
const MARKER_LIMIT: i64 = 500;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct TrendQuery {
    pub weeks: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct TrendPointView {
    pub week_end: DateTime<Utc>,
    pub k: i64,
    pub n: i64,
    pub judges: serde_json::Value,
}

impl From<WeeklyShare> for TrendPointView {
    fn from(p: WeeklyShare) -> Self {
        TrendPointView {
            week_end: p.week_end,
            k: p.k,
            n: p.n,
            judges: p.judges,
        }
    }
}

async fn markers_since(
    persist: &crate::store::persistence::PersistHandle,
    since: chrono::DateTime<Utc>,
) -> Result<Vec<ChangeLogRow>, crate::store::persistence::PersistError> {
    let mut rows = Vec::new();
    for kind in MARKER_KINDS {
        let filter = ChangeLogFilter {
            subject_kind: Some(kind.to_owned()),
            since: Some(since),
            limit: MARKER_LIMIT,
            ..ChangeLogFilter::default()
        };
        rows.extend(persist.list_config_changes(&filter).await?);
    }
    rows.sort_by_key(|r| r.changed_at);
    Ok(rows)
}

pub(crate) async fn get_trend(
    State(state): State<AppState>,
    Query(q): Query<TrendQuery>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let weeks = q.weeks.unwrap_or(8).clamp(1, 52);
    let now = Utc::now();
    let since = now - Duration::weeks(weeks);
    let points: Vec<TrendPointView> = match persist.list_weekly_corrected_share(now, weeks).await {
        Ok(rows) => rows.into_iter().map(TrendPointView::from).collect(),
        Err(e) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("persistence error: {e}"),
            );
        }
    };
    let markers = match markers_since(persist, since).await {
        Ok(rows) => rows,
        Err(e) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("persistence error: {e}"),
            );
        }
    };
    Json(json!({"points": points, "markers": markers})).into_response()
}
