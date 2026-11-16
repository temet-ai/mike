use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::extract::request::derive_session_id;
use crate::llm::decisions::record::ReviewWrite;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct OutcomeRequest {
    pub label: String,
    pub reason: String,
    #[serde(default)]
    pub evidence: Option<Value>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct OutcomeWritten {
    pub outcome_id: i64,
    pub record_id: i64,
    pub label: String,
    pub label_source: &'static str,
}

type Refusal = (StatusCode, Json<Value>);

fn refuse(status: StatusCode, error: impl Into<String>) -> Refusal {
    (status, Json(json!({ "error": error.into() })))
}

pub fn review_evidence(
    request: &OutcomeRequest,
    reviewer: Option<String>,
) -> Result<Value, Refusal> {
    if !matches!(request.label.as_str(), "right" | "wrong") {
        return Err(refuse(
            StatusCode::BAD_REQUEST,
            "label must be \"right\" or \"wrong\"",
        ));
    }
    if request.reason.trim().is_empty() {
        return Err(refuse(StatusCode::BAD_REQUEST, "reason must not be empty"));
    }
    let mut evidence = match &request.evidence {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(map)) => map.clone(),
        Some(_) => {
            return Err(refuse(
                StatusCode::BAD_REQUEST,
                "evidence must be a JSON object",
            ));
        }
    };
    evidence.insert("reason".to_owned(), Value::String(request.reason.clone()));
    evidence.insert(
        "reviewer".to_owned(),
        reviewer.map_or(Value::Null, Value::String),
    );
    Ok(Value::Object(evidence))
}

pub async fn post_outcome(
    State(state): State<AppState>,
    Path(record_id): Path<i64>,
    headers: HeaderMap,
    Json(request): Json<OutcomeRequest>,
) -> Result<Json<OutcomeWritten>, Refusal> {
    let evidence = review_evidence(&request, derive_session_id(&headers).ok())?;
    let Some(persist) = state.persist() else {
        return Err(refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence is disabled; decisions are not recorded",
        ));
    };
    let written = crate::llm::decisions::store(persist)
        .insert_review_outcome(record_id, &request.label, &evidence)
        .await
        .map_err(|e| refuse(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match written {
        ReviewWrite::Inserted(outcome_id) => Ok(Json(OutcomeWritten {
            outcome_id,
            record_id,
            label: request.label,
            label_source: crate::llm::decisions::record::CLAUDE_REVIEW,
        })),
        ReviewWrite::UnknownRecord => Err(refuse(
            StatusCode::NOT_FOUND,
            format!("no decision record {record_id}"),
        )),
        ReviewWrite::AlreadyReviewed => Err(refuse(
            StatusCode::CONFLICT,
            format!("decision record {record_id} already has a claude_review label"),
        )),
    }
}

#[cfg(test)]
mod tests;
