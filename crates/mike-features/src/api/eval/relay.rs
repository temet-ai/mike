use crate::store::persistence::eval_repo::EvalPersist as _;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use super::labels::{SubjectBody, error, unavailable, validate_entry, write_error};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_labels::NewRelayed;

const MIN_QUOTE: usize = 8;

#[derive(Debug, Deserialize)]
pub struct RelayBody {
    subject_kind: String,
    turn_id: Option<i64>,
    session_id: Option<uuid::Uuid>,
    axis: String,
    label: String,
    operator_quote: String,
    quote_session_id: Option<uuid::Uuid>,
    relayed_by_session_id: Option<uuid::Uuid>,
    note: Option<String>,
}

fn subject_of(body: &RelayBody) -> Result<SubjectBody, String> {
    match (body.subject_kind.as_str(), body.turn_id, body.session_id) {
        ("turn", Some(turn_id), _) => Ok(SubjectBody::Turn { turn_id }),
        ("session_end", _, Some(session_id)) => Ok(SubjectBody::SessionEnd { session_id }),
        ("turn", None, _) => Err("subject_kind turn needs turn_id".to_owned()),
        ("session_end", _, None) => Err("subject_kind session_end needs session_id".to_owned()),
        (other, _, _) => Err(format!(
            "subject_kind must be turn or session_end, got {other:?}"
        )),
    }
}

async fn verified_relayer(
    persist: &PersistHandle,
    quote: &str,
    claimed: Option<uuid::Uuid>,
) -> (Option<String>, bool) {
    let callers = persist
        .eval()
        .recent_relay_callers(quote)
        .await
        .unwrap_or_default();
    let claimed_text = claimed.map(|c| c.to_string());
    match (callers.as_slice(), claimed_text) {
        ([only], None) => (Some(only.clone()), true),
        ([only], Some(claimed)) => (Some(claimed.clone()), *only == claimed),
        (many, Some(claimed)) => (Some(claimed.clone()), many.contains(&claimed)),
        (_, None) => (None, false),
    }
}

async fn check_enabled(persist: &PersistHandle) -> Result<(), Response> {
    match crate::eval::labels::relay_enabled(persist).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(error(
            StatusCode::FORBIDDEN,
            "relayed labels are switched off in /settings (eval.relay_labels_enabled)",
        )),
        Err(e) => Err(error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

async fn validated(
    persist: &PersistHandle,
    body: &RelayBody,
) -> Result<(SubjectBody, uuid::Uuid), Response> {
    check_enabled(persist).await?;
    if body.operator_quote.trim().chars().count() < MIN_QUOTE {
        return Err(error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "operator_quote must be at least {MIN_QUOTE} characters of the operator's own words"
            ),
        ));
    }
    let subject = subject_of(body).map_err(|m| error(StatusCode::BAD_REQUEST, m))?;
    let vocab = persist
        .eval()
        .eval_vocabulary()
        .await
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    validate_entry(&vocab, subject.kind(), &body.axis, &body.label)
        .map_err(|m| error(StatusCode::BAD_REQUEST, m))?;
    let quote_session = body
        .quote_session_id
        .or(body.relayed_by_session_id)
        .ok_or_else(|| {
            error(
                StatusCode::BAD_REQUEST,
                "quote_session_id or relayed_by_session_id is required",
            )
        })?;
    Ok((subject, quote_session))
}

async fn quote_turn(
    persist: &PersistHandle,
    session: uuid::Uuid,
    quote: &str,
) -> Result<i64, Response> {
    match persist.eval().find_quote_turn(session, quote).await {
        Ok(Some(turn)) => Ok(turn),
        Ok(None) => Err(error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "the quoted words are not in any captured prompt of session {session}; relay the operator's exact words, and pass quote_session_id if they were said in another session"
            ),
        )),
        Err(e) => Err(error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

pub async fn post_relay_label(
    State(state): State<AppState>,
    Json(body): Json<RelayBody>,
) -> Response {
    let Some(persist) = state.persist() else {
        return unavailable();
    };
    let (subject, quote_session) = match validated(persist, &body).await {
        Ok(v) => v,
        Err(response) => return response,
    };
    let quote = body.operator_quote.trim();
    let quote_turn = match quote_turn(persist, quote_session, quote).await {
        Ok(turn) => turn,
        Err(response) => return response,
    };
    let (relayed_by, verified) = verified_relayer(persist, quote, body.relayed_by_session_id).await;
    let write = NewRelayed {
        subject: subject.subject(),
        axis: &body.axis,
        label: &body.label,
        note: body.note.as_deref(),
        provenance: json!({
            "operator_quote": quote,
            "quote_session_uuid": quote_session,
            "quote_turn_id": quote_turn,
            "relayed_by_session_uuid": relayed_by,
            "relayed_by_verified": verified,
        }),
        holdout_percent: crate::eval::labels::holdout_percent(persist)
            .await
            .unwrap_or(30),
    };
    match persist.eval().write_relayed_label(&write).await {
        Ok(label) => (
            StatusCode::CREATED,
            Json(json!({
                "label": label,
                "message": "Recorded as a relayed label. It is not gold: the operator confirms it in the mike UI.",
            })),
        )
            .into_response(),
        Err(e) => write_error(&e),
    }
}
