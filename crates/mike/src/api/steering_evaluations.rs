use crate::store::persistence::steerer_repo::SteererPersist as _;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::state::AppState;
use crate::store::persistence::steering_evaluations::{
    EvaluationFilter, EvaluationListRow, EvaluationStatsFilter, EvaluationStatsRow,
};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;
const WILSON_Z: f64 = 1.96;

#[derive(Debug, Deserialize)]
pub(crate) struct EvaluationListQuery {
    pub(crate) steerer_id: Option<i64>,
    pub(crate) question: Option<String>,
    pub(crate) mode: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) limit: Option<i64>,
    pub(crate) before_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct EvaluationPage {
    pub(crate) session_uuid: uuid::Uuid,
    pub(crate) total_count: i64,
    pub(crate) evaluations: Vec<EvaluationListRow>,
    pub(crate) next_before_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EvaluationStatsQuery {
    pub(crate) steerer_id: Option<i64>,
    pub(crate) question: Option<String>,
    pub(crate) mode: Option<String>,
    pub(crate) since: Option<DateTime<Utc>>,
    pub(crate) until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub(crate) struct Interval {
    pub(crate) low: f64,
    pub(crate) high: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct EvaluationStatsDto {
    #[serde(flatten)]
    pub(crate) row: EvaluationStatsRow,
    pub(crate) precision: Option<f64>,
    pub(crate) precision_low: Option<f64>,
    pub(crate) precision_high: Option<f64>,
    pub(crate) recall: Option<f64>,
    pub(crate) recall_low: Option<f64>,
    pub(crate) recall_high: Option<f64>,
}

#[derive(Debug, Serialize)]
pub(crate) struct EvaluationStatsPage {
    pub(crate) since: DateTime<Utc>,
    pub(crate) until: Option<DateTime<Utc>>,
    pub(crate) stats: Vec<EvaluationStatsDto>,
}

fn error(status: StatusCode, message: String) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

fn unavailable() -> Response {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "session persistence is disabled; evaluations are not available".to_owned(),
    )
}

pub(crate) fn wilson(successes: i64, total: i64) -> Option<(f64, Interval)> {
    if total <= 0 {
        return None;
    }
    let n = total as f64;
    let p = successes as f64 / n;
    let z2 = WILSON_Z * WILSON_Z;
    let centre = (p + z2 / (2.0 * n)) / (1.0 + z2 / n);
    let half = WILSON_Z * ((p * (1.0 - p) / n) + z2 / (4.0 * n * n)).sqrt() / (1.0 + z2 / n);
    Some((
        p,
        Interval {
            low: (centre - half).max(0.0),
            high: (centre + half).min(1.0),
        },
    ))
}

fn stats_dto(row: EvaluationStatsRow) -> EvaluationStatsDto {
    let precision = wilson(row.tp, row.tp + row.fp);
    let recall = wilson(row.tp, row.tp + row.false_negatives);
    EvaluationStatsDto {
        precision: precision.map(|(p, _)| p),
        precision_low: precision.map(|(_, i)| i.low),
        precision_high: precision.map(|(_, i)| i.high),
        recall: recall.map(|(r, _)| r),
        recall_low: recall.map(|(_, i)| i.low),
        recall_high: recall.map(|(_, i)| i.high),
        row,
    }
}

pub(crate) async fn list_session_evaluations(
    State(state): State<AppState>,
    Path(session_uuid): Path<uuid::Uuid>,
    Query(query): Query<EvaluationListQuery>,
) -> Response {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return error(
            StatusCode::BAD_REQUEST,
            format!("limit must be between 1 and {MAX_LIMIT}, got {limit}"),
        );
    }
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let filter = EvaluationFilter {
        session_uuid,
        steerer_id: query.steerer_id,
        question: query.question,
        mode: query.mode,
        status: query.status,
        before_id: query.before_id,
        limit: limit + 1,
    };
    let (mut rows, total_count) = match persist.steerer().list_session_evaluations(&filter).await {
        Ok(page) => page,
        Err(e) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("persistence error: {e}"),
            );
        }
    };
    let total_count = if rows.is_empty() && query.before_id.is_some() {
        persist
            .steerer()
            .count_session_evaluations(&filter)
            .await
            .unwrap_or(total_count)
    } else {
        total_count
    };
    let has_more = rows.len() > usize::try_from(limit).unwrap_or(0);
    rows.truncate(usize::try_from(limit).unwrap_or(0));
    let next_before_id = if has_more {
        rows.last().map(|r| r.id)
    } else {
        None
    };
    Json(EvaluationPage {
        session_uuid,
        total_count,
        evaluations: rows,
        next_before_id,
    })
    .into_response()
}

pub(crate) async fn get_evaluation_stats(
    State(state): State<AppState>,
    Query(query): Query<EvaluationStatsQuery>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let since = query
        .since
        .unwrap_or_else(|| Utc::now() - chrono::Duration::days(14));
    let filter = EvaluationStatsFilter {
        steerer_id: query.steerer_id,
        question: query.question,
        mode: query.mode,
        since: Some(since),
        until: query.until,
    };
    match persist.steerer().evaluation_stats(&filter).await {
        Ok(rows) => Json(EvaluationStatsPage {
            since,
            until: query.until,
            stats: rows.into_iter().map(stats_dto).collect(),
        })
        .into_response(),
        Err(e) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("persistence error: {e}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::wilson;

    #[test]
    fn wilson_interval_brackets_the_point_estimate() {
        let Some((p, interval)) = wilson(8, 10) else {
            panic!("ten trials give an interval");
        };
        assert!((p - 0.8).abs() < 1e-9);
        assert!(interval.low < p && p < interval.high);
        assert!(interval.low > 0.4 && interval.high < 1.0);
    }

    #[test]
    fn wilson_is_none_without_trials() {
        assert!(wilson(0, 0).is_none());
    }
}
