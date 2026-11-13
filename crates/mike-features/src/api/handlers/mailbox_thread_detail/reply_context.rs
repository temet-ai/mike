use crate::store::persistence::launch_repo::LaunchPersist as _;
use axum::http::StatusCode;
use axum::response::Response;

use crate::api::handlers::{PostThreadMessageBody, mailbox_error};
use crate::store::persistence::PersistHandle;

pub struct ReplyContext {
    pub thread_id: i64,
    pub thread_uuid: uuid::Uuid,
    pub subject: String,
    pub from_session_uuid: Option<uuid::Uuid>,
    pub from_session_id: Option<i64>,
    pub in_reply_to_id: Option<i64>,
}

pub async fn build_reply_context(
    handle: &PersistHandle,
    thread_uuid: uuid::Uuid,
    body: &PostThreadMessageBody,
) -> Result<ReplyContext, Response> {
    let core = match handle.launch().fetch_thread_core_by_uuid(thread_uuid).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return Err(mailbox_error(
                StatusCode::NOT_FOUND,
                "thread_not_found",
                format!("no thread with uuid {thread_uuid}"),
            ));
        }
        Err(e) => {
            tracing::warn!(error = %e, %thread_uuid, "post_thread_message: thread lookup failed");
            return Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to fetch thread".to_string(),
            ));
        }
    };

    let in_reply_to_id =
        resolve_in_reply_to(handle, thread_uuid, core.id, body.in_reply_to.as_deref()).await?;
    let (from_session_uuid, from_session_id) =
        resolve_from_session(handle, body.from_session_id.as_deref()).await?;

    Ok(ReplyContext {
        thread_id: core.id,
        thread_uuid: core.thread_uuid,
        subject: core.subject,
        from_session_uuid,
        from_session_id,
        in_reply_to_id,
    })
}

async fn resolve_in_reply_to(
    handle: &PersistHandle,
    thread_uuid: uuid::Uuid,
    thread_id: i64,
    in_reply_to: Option<&str>,
) -> Result<Option<i64>, Response> {
    let Some(raw) = in_reply_to else {
        return Ok(None);
    };
    let parent_uuid = uuid::Uuid::parse_str(raw).map_err(|_| {
        mailbox_error(
            StatusCode::BAD_REQUEST,
            "invalid_in_reply_to",
            format!("in_reply_to is not a valid UUID: {raw}"),
        )
    })?;
    match handle
        .launch()
        .fetch_message_core_by_uuid(parent_uuid)
        .await
    {
        Ok(Some(parent)) if parent.thread_id == Some(thread_id) => Ok(Some(parent.id)),
        Ok(_) => Err(mailbox_error(
            StatusCode::BAD_REQUEST,
            "invalid_in_reply_to",
            format!("message {parent_uuid} is not in thread {thread_uuid}"),
        )),
        Err(e) => {
            tracing::warn!(error = %e, %thread_uuid, "post_thread_message: in_reply_to lookup failed");
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to resolve in_reply_to".to_string(),
            ))
        }
    }
}

async fn resolve_from_session(
    handle: &PersistHandle,
    from_session_id: Option<&str>,
) -> Result<(Option<uuid::Uuid>, Option<i64>), Response> {
    let Some(raw) = from_session_id else {
        return Ok((None, None));
    };
    let from_session_uuid = uuid::Uuid::parse_str(raw).map_err(|_| {
        mailbox_error(
            StatusCode::BAD_REQUEST,
            "invalid_from_session_id",
            format!("from_session_id is not a valid UUID: {raw}"),
        )
    })?;
    match handle
        .launch()
        .resolve_session_core(from_session_uuid)
        .await
    {
        Ok(Some((id, _))) => Ok((Some(from_session_uuid), Some(id))),
        Ok(None) => Err(mailbox_error(
            StatusCode::NOT_FOUND,
            "from_session_not_found",
            format!("no session with uuid {from_session_uuid}"),
        )),
        Err(e) => {
            tracing::warn!(error = %e, %from_session_uuid, "post_thread_message: sender lookup failed");
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to resolve sender session".to_string(),
            ))
        }
    }
}
