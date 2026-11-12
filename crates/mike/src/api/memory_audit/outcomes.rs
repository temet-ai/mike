use axum::Json;
use axum::extract::State;
use axum::response::Response;
use serde::Serialize;

use crate::state::AppState;
use crate::store::persistence::lesson_injections::LessonOutcomeRow;
use crate::web_base::{persist_error_response, persist_unavailable_response};

const OUTCOME_CODES: [&str; 4] = ["success", "partial", "failure", "unknown"];

#[derive(Debug, Serialize)]
pub(crate) struct LessonOutcomeSummaryDto {
    pub(crate) lesson_id: i64,
    pub(crate) lesson_text: String,
    pub(crate) injection_count: i64,
    pub(crate) distinct_session_count: i64,
    pub(crate) outcome_counts: std::collections::BTreeMap<String, i64>,
    pub(crate) mean_quality_score: Option<f64>,
}

fn push_new_lesson_summary_if_needed(
    summaries: &mut Vec<LessonOutcomeSummaryDto>,
    row: &LessonOutcomeRow,
) {
    let is_new_lesson = summaries
        .last()
        .is_none_or(|last| last.lesson_id != row.lesson_id);
    if !is_new_lesson {
        return;
    }
    let outcome_counts = OUTCOME_CODES
        .iter()
        .map(|code| ((*code).to_string(), 0))
        .collect();
    summaries.push(LessonOutcomeSummaryDto {
        lesson_id: row.lesson_id,
        lesson_text: row.lesson_text.clone(),
        injection_count: row.injection_count,
        distinct_session_count: row.distinct_session_count,
        outcome_counts,
        mean_quality_score: row.mean_quality_score,
    });
}

pub(crate) async fn list_lesson_outcomes(
    State(state): State<AppState>,
) -> Result<Json<Vec<LessonOutcomeSummaryDto>>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let rows: Vec<LessonOutcomeRow> = handle
        .lesson_outcomes_observed_after_injection()
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "list_lesson_outcomes: query failed");
            persist_error_response(e)
        })?;

    let mut summaries: Vec<LessonOutcomeSummaryDto> = Vec::new();
    for row in &rows {
        push_new_lesson_summary_if_needed(&mut summaries, row);
        if let (Some(outcome), Some(count)) = (&row.outcome, row.outcome_count)
            && let Some(entry) = summaries.last_mut()
        {
            entry.outcome_counts.insert(outcome.clone(), count);
        }
    }
    Ok(Json(summaries))
}
