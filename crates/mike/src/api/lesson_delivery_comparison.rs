use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::AppState;
use crate::store::persistence::lesson_injections::ArmComparisonRow;

const MAX_WINDOW_DAYS: i64 = 31;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ComparisonQuery {
    from: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, PartialEq)]
pub(crate) struct ArmReport {
    arm: String,
    sessions: i64,
    operator_turns: i64,
    lessons_delivered: i64,
    lessons_per_turn: Option<f64>,
    requests: i64,
    usage_requests: i64,
    input_tokens: i64,
    cache_read_tokens: i64,
    cache_write_tokens: i64,
    cache_read_share: Option<f64>,
    judged_turns: i64,
    corrected_turns: i64,
    corrected_rate: Option<f64>,
}

fn ratio(numerator: i64, denominator: i64) -> Option<f64> {
    (denominator > 0).then(|| numerator as f64 / denominator as f64)
}

impl From<ArmComparisonRow> for ArmReport {
    fn from(row: ArmComparisonRow) -> Self {
        let total_input = row.input_tokens + row.cache_read_tokens + row.cache_write_tokens;
        Self {
            lessons_per_turn: ratio(row.lessons_delivered, row.operator_turns),
            cache_read_share: ratio(row.cache_read_tokens, total_input),
            corrected_rate: ratio(row.corrected_turns, row.judged_turns),
            arm: row.arm,
            sessions: row.sessions,
            operator_turns: row.operator_turns,
            lessons_delivered: row.lessons_delivered,
            requests: row.requests,
            usage_requests: row.usage_requests,
            input_tokens: row.input_tokens,
            cache_read_tokens: row.cache_read_tokens,
            cache_write_tokens: row.cache_write_tokens,
            judged_turns: row.judged_turns,
            corrected_turns: row.corrected_turns,
        }
    }
}

fn window(query: &ComparisonQuery) -> Result<(DateTime<Utc>, DateTime<Utc>), &'static str> {
    let until = query.until.unwrap_or_else(Utc::now);
    let from = query.from.unwrap_or(until - Duration::days(7));
    if from >= until || until - from > Duration::days(MAX_WINDOW_DAYS) {
        return Err("comparison window must be positive and at most thirty-one days");
    }
    Ok((from, until))
}

pub(crate) async fn get_delivery_comparison(
    State(state): State<AppState>,
    Query(query): Query<ComparisonQuery>,
) -> Response {
    let Some(persist) = state.persist() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "persistence is disabled").into_response();
    };
    let (from, until) = match window(&query) {
        Ok(window) => window,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    match persist.compare_lesson_delivery_arms(from, until).await {
        Ok(rows) => Json(json!({
            "from": from,
            "until": until,
            "arms": rows.into_iter().map(ArmReport::from).collect::<Vec<_>>(),
            "definitions": {
                "sessions": "sessions in the arm that made at least one request in the window",
                "operator_turns": "genuine operator turns with text, created in the window",
                "lessons_per_turn": "lessons_delivered / operator_turns; lessons_delivered counts lesson injection records of any origin written in the window",
                "cache_read_share": "cache_read_tokens / (input_tokens + cache_read_tokens + cache_write_tokens) over the arm's responses; usage_requests of requests carried usage",
                "corrected_rate": "corrected_turns / judged_turns, where judged turns were labelled accepted, corrected or redirected by the judge"
            },
            "limitations": "Arms are assigned by a hash of the session id, so sessions are comparable but not matched by project or task. A rate over a few turns is a reason to read the sessions, not a result."
        }))
        .into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(arm: &str) -> ArmComparisonRow {
        ArmComparisonRow {
            arm: arm.to_owned(),
            sessions: 3,
            operator_turns: 10,
            judged_turns: 8,
            corrected_turns: 2,
            lessons_delivered: 25,
            requests: 400,
            usage_requests: 390,
            input_tokens: 100,
            cache_read_tokens: 800,
            cache_write_tokens: 100,
        }
    }

    #[test]
    fn rates_are_computed_with_the_counts_kept_beside_them() {
        let report = ArmReport::from(row("mod"));
        assert_eq!(report.lessons_per_turn, Some(2.5));
        assert_eq!(report.cache_read_share, Some(0.8));
        assert_eq!(report.corrected_rate, Some(0.25));
        assert_eq!(report.operator_turns, 10);
        assert_eq!(report.judged_turns, 8);
        assert_eq!(report.corrected_turns, 2);
    }

    #[test]
    fn an_arm_with_no_turns_or_tokens_has_no_rates() {
        let report = ArmReport::from(ArmComparisonRow {
            arm: "control".to_owned(),
            sessions: 0,
            operator_turns: 0,
            judged_turns: 0,
            corrected_turns: 0,
            lessons_delivered: 0,
            requests: 0,
            usage_requests: 0,
            input_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        });
        assert_eq!(report.lessons_per_turn, None);
        assert_eq!(report.cache_read_share, None);
        assert_eq!(report.corrected_rate, None);
    }

    #[test]
    fn window_rejects_empty_and_unbounded_ranges() {
        let until = Utc::now();
        let query = |from, until| ComparisonQuery { from, until };
        assert!(window(&query(Some(until), Some(until))).is_err());
        assert!(window(&query(Some(until - Duration::days(32)), Some(until))).is_err());
        assert_eq!(
            window(&query(None, Some(until))).map(|(from, end)| end - from),
            Ok(Duration::days(7))
        );
    }
}
