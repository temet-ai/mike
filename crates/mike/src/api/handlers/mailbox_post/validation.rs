use axum::http::StatusCode;
use axum::response::Response;

use crate::api::handlers::{
    PostSessionMessageBody, invalid_context_items, invalid_tags, mailbox_error,
};
use crate::store::persistence::PersistHandle;

use super::record_failed_send;

pub(super) struct ParsedSend {
    pub(super) from_session_uuid: Option<uuid::Uuid>,
    pub(super) thread_id_param: Option<uuid::Uuid>,
    pub(super) in_reply_to_param: Option<uuid::Uuid>,
    pub(super) subject_param: Option<String>,
    pub(super) threading_requested: bool,
}

pub(super) async fn validate_send_request(
    handle: &PersistHandle,
    to_session_uuid: uuid::Uuid,
    body: &PostSessionMessageBody,
) -> Result<ParsedSend, Response> {
    if body.message.trim().is_empty() {
        record_failed_send(handle, None, to_session_uuid, "empty_message").await;
        return Err(mailbox_error(
            StatusCode::BAD_REQUEST,
            "empty_message",
            "message must not be empty".to_string(),
        ));
    }

    let from_session_uuid = match body.from_session_id.as_deref() {
        Some(raw) => match uuid::Uuid::parse_str(raw) {
            Ok(u) => Some(u),
            Err(_) => {
                record_failed_send(handle, None, to_session_uuid, "invalid_from_session_id").await;
                return Err(mailbox_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_from_session_id",
                    format!("from_session_id is not a valid UUID: {raw}"),
                ));
            }
        },
        None => None,
    };

    let thread_id_param = match body.thread_id.as_deref() {
        Some(raw) => match uuid::Uuid::parse_str(raw) {
            Ok(u) => Some(u),
            Err(_) => {
                record_failed_send(handle, None, to_session_uuid, "invalid_thread_id").await;
                return Err(mailbox_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_thread_id",
                    format!("thread_id is not a valid UUID: {raw}"),
                ));
            }
        },
        None => None,
    };
    let in_reply_to_param = match body.in_reply_to.as_deref() {
        Some(raw) => match uuid::Uuid::parse_str(raw) {
            Ok(u) => Some(u),
            Err(_) => {
                record_failed_send(handle, None, to_session_uuid, "invalid_in_reply_to").await;
                return Err(mailbox_error(
                    StatusCode::BAD_REQUEST,
                    "invalid_in_reply_to",
                    format!("in_reply_to is not a valid UUID: {raw}"),
                ));
            }
        },
        None => None,
    };
    let subject_param = body
        .subject
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let threading_requested =
        thread_id_param.is_some() || in_reply_to_param.is_some() || subject_param.is_some();

    if let Some((code, message)) = invalid_context_items(&body.context) {
        record_failed_send(handle, None, to_session_uuid, code).await;
        return Err(mailbox_error(StatusCode::BAD_REQUEST, code, message));
    }
    if let Some(tags) = body.tags.as_deref()
        && let Some((code, message)) = invalid_tags(tags)
    {
        record_failed_send(handle, None, to_session_uuid, code).await;
        return Err(mailbox_error(StatusCode::BAD_REQUEST, code, message));
    }

    Ok(ParsedSend {
        from_session_uuid,
        thread_id_param,
        in_reply_to_param,
        subject_param,
        threading_requested,
    })
}
