use crate::store::persistence::web_repo::WebPersist as _;
use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{bad_request, page_limit, refuse};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::wright::conversation::{
    MessageRow, MessageView, NewOperatorMessage, Posted,
};
use crate::store::persistence::wright::conversation_steers::SteerRow;
use crate::web_base::{persist_error_response, persist_unavailable_response};
use crate::wright::conversation::engineer::EngineerAnswers;
use crate::wright::conversation::steer_apply::Steers;
use crate::wright::conversation::steering::{self, UndoError};
use crate::wright::conversation::token::{self, OperatorToken};
use crate::wright::conversation::{Conversation, worker};

const MESSAGES_DEFAULT_LIMIT: i64 = 100;
const CLIENT_MSG_ID_MAX: usize = 128;
const BODY_MAX_CHARS: usize = 16 * 1024;

#[derive(Debug, Deserialize)]
pub(crate) struct MessagesQuery {
    after: Option<i64>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostBody {
    client_msg_id: String,
    body: String,
    reply_to: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UndoBody {
    version: i32,
}

fn conflict(reason: &str) -> Response {
    (
        StatusCode::CONFLICT,
        Json(json!({"error": "conflict", "reason": reason})),
    )
        .into_response()
}

fn unauthorized(reason: &str) -> Response {
    let mut response = (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": "unauthorized", "reason": reason})),
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    response
}

fn operator_token() -> Option<OperatorToken> {
    let path = token::default_path().ok()?;
    token::load(&path).ok().flatten()
}

pub(crate) fn require_operator(headers: &HeaderMap) -> Result<(), Response> {
    token::authorize(headers, operator_token().as_ref()).map_err(|reason| unauthorized(&reason))
}

fn parse_json<T: serde::de::DeserializeOwned>(bytes: &Bytes) -> Result<T, Response> {
    serde_json::from_slice(bytes).map_err(|e| {
        bad_request(format!(
            "the request body is not the expected JSON (line {}, column {}): {e}; body was `{}`",
            e.line(),
            e.column(),
            String::from_utf8_lossy(bytes)
        ))
    })
}

fn steer_json(view: &MessageView) -> Value {
    let Some(id) = view.steer_id else {
        return Value::Null;
    };
    let can_undo = view.steer_applied_at.is_some()
        && view.steer_undone_at.is_none()
        && view.steer_refusal.is_none();
    json!({
        "id": id,
        "change": view.steer_change,
        "applied_at": view.steer_applied_at,
        "undone_at": view.steer_undone_at,
        "can_undo": can_undo,
        "version": view.steer_version,
        "refused": view.steer_refusal,
    })
}

fn message_json(view: &MessageView) -> Value {
    json!({
        "id": view.id,
        "author": view.author,
        "body": view.body,
        "reply_to": view.reply_to,
        "links": view.links,
        "created_at": view.created_at,
        "steer": steer_json(view),
    })
}

fn posted_json(row: &MessageRow) -> Value {
    json!({
        "id": row.id,
        "author": row.author,
        "body": row.body,
        "reply_to": row.reply_to,
        "links": row.links,
        "created_at": row.created_at,
        "steer": Value::Null,
    })
}

pub(crate) fn steer_row_json(row: &SteerRow) -> Value {
    json!({
        "id": row.id,
        "change": row.change,
        "applied_at": row.applied_at,
        "undone_at": row.undone_at,
        "can_undo": row.applied_at.is_some() && row.undone_at.is_none() && row.refusal.is_none(),
        "version": row.version,
        "refused": row.refusal,
    })
}

pub(crate) async fn list_messages(
    State(state): State<AppState>,
    Query(query): Query<MessagesQuery>,
) -> Result<Json<Value>, Response> {
    let handle = state.persist().ok_or_else(persist_unavailable_response)?;
    list_messages_with(handle, &query).await.map(Json)
}

pub(crate) async fn list_messages_with(
    handle: &PersistHandle,
    query: &MessagesQuery,
) -> Result<Value, Response> {
    let after = query.after.unwrap_or(0);
    if after < 0 {
        return Err(bad_request(format!(
            "after is {after}; it must be 0 or more"
        )));
    }
    let limit = page_limit(query.limit, MESSAGES_DEFAULT_LIMIT, "limit").map_err(bad_request)?;
    let web = handle.web();
    let views = web
        .list_wright_messages(after, limit)
        .await
        .map_err(persist_error_response)?;
    let pending = web
        .wright_reply_pending()
        .await
        .map_err(persist_error_response)?;
    Ok(json!({
        "messages": views.iter().map(message_json).collect::<Vec<_>>(),
        "pending_reply": pending,
    }))
}

fn validate(body: &PostBody) -> Result<(), String> {
    let id = body.client_msg_id.trim();
    if id.is_empty() || id.chars().count() > CLIENT_MSG_ID_MAX || id != body.client_msg_id {
        return Err(format!(
            "client_msg_id is `{}`; it must be 1 to {CLIENT_MSG_ID_MAX} characters with no surrounding spaces",
            body.client_msg_id
        ));
    }
    let chars = body.body.trim().chars().count();
    if chars == 0 || chars > BODY_MAX_CHARS {
        return Err(format!(
            "body has {chars} characters after trimming; it must have 1 to {BODY_MAX_CHARS}"
        ));
    }
    if let Some(reply_to) = body.reply_to.filter(|id| *id < 1) {
        return Err(format!("reply_to is {reply_to}; a message id is 1 or more"));
    }
    Ok(())
}

pub(crate) async fn post_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Response {
    let Some(handle) = state.persist() else {
        return persist_unavailable_response();
    };
    match post_message_with(handle, operator_token().as_ref(), &headers, &bytes).await {
        Ok(response) | Err(response) => response,
    }
}

pub(crate) async fn post_message_with(
    handle: &PersistHandle,
    expected: Option<&OperatorToken>,
    headers: &HeaderMap,
    bytes: &Bytes,
) -> Result<Response, Response> {
    token::authorize(headers, expected).map_err(|reason| unauthorized(&reason))?;
    let body: PostBody = parse_json(bytes)?;
    validate(&body).map_err(bad_request)?;
    let web = handle.web();
    if let Some(reply_to) = body.reply_to
        && !web
            .wright_message_exists(reply_to)
            .await
            .map_err(persist_error_response)?
    {
        return Err(bad_request(format!(
            "reply_to is {reply_to}; no message in the conversation has that id"
        )));
    }
    let posted = web
        .post_wright_message(&NewOperatorMessage {
            client_msg_id: &body.client_msg_id,
            body: body.body.trim(),
            reply_to: body.reply_to,
        })
        .await
        .map_err(persist_error_response)?;
    match posted {
        Posted::Created(row) => {
            worker::wake();
            Ok((
                StatusCode::CREATED,
                Json(json!({"message": posted_json(&row), "created": true})),
            )
                .into_response())
        }
        Posted::Existing(row) if row.body == body.body.trim() && row.reply_to == body.reply_to => {
            Ok((
                StatusCode::OK,
                Json(json!({"message": posted_json(&row), "created": false})),
            )
                .into_response())
        }
        Posted::Existing(row) => Err(conflict(&format!(
            "client_msg_id `{}` already holds message {} with a different body or reply_to; send a new client_msg_id for a new message",
            body.client_msg_id, row.id
        ))),
    }
}

pub(crate) async fn undo_steer(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Response {
    let Some(handle) = state.persist().cloned() else {
        return persist_unavailable_response();
    };
    let convo = Conversation {
        steers: Arc::new(Steers {
            persist: handle.clone(),
        }),
        engine: Arc::new(EngineerAnswers::for_state(&state)),
        persist: handle,
    };
    match undo_steer_with(&convo, operator_token().as_ref(), &headers, id, &bytes).await {
        Ok(response) | Err(response) => response,
    }
}

pub(crate) async fn undo_steer_with(
    convo: &Conversation,
    expected: Option<&OperatorToken>,
    headers: &HeaderMap,
    id: i64,
    bytes: &Bytes,
) -> Result<Response, Response> {
    token::authorize(headers, expected).map_err(|reason| unauthorized(&reason))?;
    let body: UndoBody = parse_json(bytes)?;
    match steering::undo(convo, id, body.version).await {
        Ok(row) => {
            Ok((StatusCode::OK, Json(json!({"steer": steer_row_json(&row)}))).into_response())
        }
        Err(UndoError::NotFound(id)) => Err(refuse(
            StatusCode::NOT_FOUND,
            &format!("no Wright steer has id {id}"),
        )),
        Err(UndoError::Conflict(reason)) => Err(conflict(&reason)),
        Err(UndoError::Failed(reason)) => Err(refuse(StatusCode::INTERNAL_SERVER_ERROR, &reason)),
    }
}
