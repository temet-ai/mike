use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::state::AppState;
use crate::store::persistence::lessons::skill_live::SkillLiveOutcome;

const MAX_OUTCOMES: usize = 500;

#[derive(Debug, Deserialize)]
pub(crate) struct OutcomesRequest {
    outcomes: Vec<SkillLiveOutcome>,
}

fn refusal(request: &OutcomesRequest) -> Option<String> {
    if request.outcomes.len() > MAX_OUTCOMES {
        return Some(format!(
            "{} outcomes sent, at most {MAX_OUTCOMES} per request",
            request.outcomes.len()
        ));
    }
    request.outcomes.iter().find_map(|outcome| {
        outcome
            .inconsistency()
            .map(|problem| format!("{} / {}: {problem}", outcome.project, outcome.folder))
    })
}

pub(crate) async fn post_skill_outcomes(
    State(state): State<AppState>,
    Json(request): Json<OutcomesRequest>,
) -> Response {
    let Some(persist) = state.persist() else {
        return (StatusCode::SERVICE_UNAVAILABLE, "persistence is disabled").into_response();
    };
    if let Some(problem) = refusal(&request) {
        return (StatusCode::UNPROCESSABLE_ENTITY, problem).into_response();
    }
    match persist.upsert_skill_live_outcomes(&request.outcomes).await {
        Ok(stored) => {
            Json(json!({"sent": request.outcomes.len(), "stored": stored})).into_response()
        }
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(value: serde_json::Value) -> anyhow::Result<OutcomesRequest> {
        Ok(serde_json::from_value(value)?)
    }

    fn outcome(loaded: i32, judged: i32, corrected: i32) -> serde_json::Value {
        json!({
            "project": "11111111-0000-4000-8000-000000000000",
            "folder": "deploy-and-restart",
            "first_shipped_on": "2026-10-14",
            "window_start": "2026-10-14T00:00:00Z",
            "window_end": "2026-10-15T00:00:00Z",
            "loads": 12,
            "load_sessions": 4,
            "loaded_turns": loaded,
            "judged_turns": judged,
            "corrected_turns": corrected,
            "base_judged_turns": 40,
            "base_corrected_turns": 8
        })
    }

    #[test]
    fn consistent_numbers_are_accepted() -> anyhow::Result<()> {
        let parsed = request(json!({"outcomes": [outcome(9, 7, 3)]}))?;
        assert_eq!(refusal(&parsed), None);
        Ok(())
    }

    #[test]
    fn more_corrected_than_judged_turns_is_refused_with_the_skill_named() -> anyhow::Result<()> {
        let parsed = request(json!({"outcomes": [outcome(9, 7, 8)]}))?;
        let problem = refusal(&parsed).ok_or_else(|| anyhow::anyhow!("must be refused"))?;
        assert!(problem.contains("deploy-and-restart"), "{problem}");
        assert!(problem.contains("corrected_turns is 8"), "{problem}");
        Ok(())
    }

    #[test]
    fn a_retirement_needs_both_its_time_and_its_reason() -> anyhow::Result<()> {
        let mut half = outcome(9, 7, 3);
        half["retired_at"] = json!("2026-10-15T00:00:00Z");
        let parsed = request(json!({"outcomes": [half]}))?;
        assert!(refusal(&parsed).is_some_and(|p| p.contains("together")));
        let mut whole = outcome(9, 7, 3);
        whole["retired_at"] = json!("2026-10-15T00:00:00Z");
        whole["retired_reason"] = json!("corrected_above_base");
        assert_eq!(refusal(&request(json!({"outcomes": [whole]}))?), None);
        Ok(())
    }

    #[test]
    fn an_unknown_retirement_reason_does_not_parse() {
        let mut bad = outcome(9, 7, 3);
        bad["retired_at"] = json!("2026-10-15T00:00:00Z");
        bad["retired_reason"] = json!("disliked");
        assert!(request(json!({"outcomes": [bad]})).is_err());
    }

    #[test]
    fn a_request_over_the_limit_is_refused() -> anyhow::Result<()> {
        let many: Vec<_> = (0..=MAX_OUTCOMES).map(|_| outcome(9, 7, 3)).collect();
        let parsed = request(json!({"outcomes": many}))?;
        assert!(refusal(&parsed).is_some_and(|p| p.contains("at most 500")));
        Ok(())
    }
}
