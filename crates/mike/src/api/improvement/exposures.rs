use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::state::AppState;

#[derive(Default, Deserialize)]
pub(crate) struct ExposureQuery {
    project: Option<String>,
    from: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
}

fn window(q: &ExposureQuery) -> Result<(DateTime<Utc>, DateTime<Utc>), &'static str> {
    let until = q.until.unwrap_or_else(Utc::now);
    let from = q.from.unwrap_or(until - Duration::days(7));
    if from >= until || until - from > Duration::days(7) {
        return Err("exposure window must be positive and at most seven days");
    }
    Ok((from, until))
}

pub(crate) async fn list_exposures(
    State(state): State<AppState>,
    Query(q): Query<ExposureQuery>,
) -> Response {
    let Some(persist) = state.persist() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "persistence is disabled").into_response();
    };
    let (from, until) = match window(&q) {
        Ok(window) => window,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    match persist.list_lesson_exposures(q.project.as_deref(), from, until).await {
        Ok(rows) => Json(json!({
            "from": from, "until": until, "lessons": rows,
            "measure": "recorded_exposure_association",
            "limitations": "Only primary requests linked to a turn of their own session contribute outcomes and usage. Unattributed injections include foreign-session links and agent lanes. Injection records are not proof of model receipt or compliance. Work shared by multiple lessons must not be summed. Whole-turn failed checks may precede injection. Outcomes are observational, not causal."
        })).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposure_window_rejects_empty_and_unbounded_ranges() {
        let until = Utc::now();
        assert!(
            window(&ExposureQuery {
                from: Some(until),
                until: Some(until),
                project: None
            })
            .is_err()
        );
        assert!(
            window(&ExposureQuery {
                from: Some(until - Duration::days(8)),
                until: Some(until),
                project: None
            })
            .is_err()
        );
        assert_eq!(
            window(&ExposureQuery {
                until: Some(until),
                ..Default::default()
            })
            .map(|(from, end)| end - from),
            Ok(Duration::days(7))
        );
    }
}
