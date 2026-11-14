use std::collections::BTreeMap;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::{Asked, Decider};
use crate::llm::decisions::record::DecisionSubject;
use crate::state::AppState;

pub const EXTERNAL_POINTS: &[(&str, &str)] = &[("docs_agent", "docs_agent_v1")];

#[derive(Debug, Deserialize)]
pub struct AskRequest {
    pub point: String,
    pub question_set: String,
    pub state: Value,
    pub questions: BTreeMap<String, Question>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct AskAnswer {
    pub decided_by: &'static str,
    pub record_id: Option<i64>,
    pub answers: Option<Value>,
    pub error: Option<String>,
}

pub fn external_point(point: &str, question_set: &str) -> Option<(&'static str, &'static str)> {
    EXTERNAL_POINTS
        .iter()
        .copied()
        .find(|(p, q)| *p == point && *q == question_set)
}

pub fn answer_of(asked: &Asked, applies: bool) -> AskAnswer {
    let decided = applies && !asked.failed();
    AskAnswer {
        decided_by: if decided { "jev" } else { "off" },
        record_id: asked.record_id,
        answers: decided
            .then(|| serde_json::to_value(&asked.answers).ok())
            .flatten(),
        error: asked.error.clone(),
    }
}

fn off() -> AskAnswer {
    AskAnswer {
        decided_by: "off",
        record_id: None,
        answers: None,
        error: None,
    }
}

async fn ask(decider: &Decider, question_set: &'static str, request: AskRequest) -> AskAnswer {
    let asked = decider
        .ask(
            question_set,
            request.state,
            request.questions,
            DecisionSubject::default(),
            Value::Object(serde_json::Map::new()),
        )
        .await;
    let answer = answer_of(&asked, decider.applies());
    let applied = if answer.decided_by == "jev" {
        "chosen"
    } else {
        "none"
    };
    decider
        .mark(
            &asked,
            answer.answers.clone().unwrap_or(Value::Null),
            applied,
        )
        .await;
    answer
}

pub async fn post_ask(
    State(state): State<AppState>,
    Json(request): Json<AskRequest>,
) -> Result<Json<AskAnswer>, (StatusCode, Json<Value>)> {
    let Some((point, question_set)) = external_point(&request.point, &request.question_set) else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!(
                    "`{}` with question set `{}` is not a decision point callers outside mike may ask",
                    request.point, request.question_set
                ),
                "allowed": EXTERNAL_POINTS,
            })),
        ));
    };
    let Some(decider) = state.decision_env().and_then(|env| env.decider(point)) else {
        return Ok(Json(off()));
    };
    Ok(Json(ask(&decider, question_set, request).await))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::decisions::Answer;

    #[test]
    fn only_listed_points_with_their_question_set_may_be_asked() {
        assert_eq!(
            external_point("docs_agent", "docs_agent_v1"),
            Some(("docs_agent", "docs_agent_v1"))
        );
        assert_eq!(external_point("docs_agent", "other_v1"), None);
        assert_eq!(external_point("turn_judge", "docs_agent_v1"), None);
    }

    #[test]
    fn answers_come_back_only_when_jev_decided_on_a_point_that_applies() {
        let asked = Asked::answered(BTreeMap::from([(
            "schema".to_owned(),
            Answer::Noul {
                noul: 0.8,
                confidence: None,
            },
        )]));
        let on = answer_of(&asked, true);
        assert_eq!(on.decided_by, "jev");
        assert!(on.answers.is_some());
        let shadow = answer_of(&asked, false);
        assert_eq!(shadow.decided_by, "off");
        assert!(shadow.answers.is_none());
        let failed = answer_of(&Asked::failed_with("timeout"), true);
        assert_eq!(failed.decided_by, "off");
        assert_eq!(failed.error.as_deref(), Some("timeout"));
    }
}
