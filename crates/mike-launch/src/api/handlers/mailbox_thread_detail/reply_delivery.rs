use crate::store::persistence::launch_repo::LaunchPersist as _;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::api::handlers::{
    FailedDeliveryDto, PostThreadMessageBody, ThreadDeliveriesResponse, ThreadDeliveryDto,
    mailbox_error,
};
use crate::base_state::BaseState;
use crate::store::persistence::PersistHandle;

use super::reply_context::ReplyContext;

pub async fn deliver_post_only(
    handle: &PersistHandle,
    ctx: &ReplyContext,
    body: &PostThreadMessageBody,
) -> Result<(StatusCode, Json<ThreadDeliveriesResponse>), Response> {
    let post_message_uuid = uuid::Uuid::new_v4();
    if let Err(e) = handle
        .launch()
        .insert_threaded_message_with_context(
            crate::store::persistence::mailbox::ThreadedMessageInsert {
                message_uuid: post_message_uuid,
                to_session_id: None,
                from_session_id: ctx.from_session_id,
                body: &body.body,
                thread_id: Some(ctx.thread_id),
                in_reply_to_id: ctx.in_reply_to_id,
            },
            &body.context,
        )
        .await
    {
        tracing::warn!(error = %e, thread_uuid = %ctx.thread_uuid, "post_thread_message: post-only insert failed");
        return Err(mailbox_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "database_error",
            "failed to post to thread".to_string(),
        ));
    }
    touch_thread_best_effort(handle, ctx.thread_id, ctx.thread_uuid).await;
    Ok((
        StatusCode::CREATED,
        Json(ThreadDeliveriesResponse {
            thread_uuid: ctx.thread_uuid,
            subject: ctx.subject.clone(),
            deliveries: Vec::new(),
            failed: Vec::new(),
            post_message_uuid: Some(post_message_uuid),
        }),
    ))
}

pub async fn deliver_to_recipients(
    handle: &PersistHandle,
    state: &BaseState,
    ctx: &ReplyContext,
    recipient_uuids: &[uuid::Uuid],
    body: &PostThreadMessageBody,
) -> Result<(StatusCode, Json<ThreadDeliveriesResponse>), Response> {
    let mut deliveries = Vec::with_capacity(recipient_uuids.len());
    let mut failed = Vec::new();
    for to_uuid in recipient_uuids {
        match deliver_one_recipient(handle, state, ctx, *to_uuid, body).await {
            RecipientOutcome::Delivered(dto) => deliveries.push(dto),
            RecipientOutcome::Failed(dto) => failed.push(dto),
        }
    }

    if deliveries.is_empty() {
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ThreadDeliveriesResponse {
                thread_uuid: ctx.thread_uuid,
                subject: ctx.subject.clone(),
                deliveries,
                failed,
                post_message_uuid: None,
            }),
        )
            .into_response());
    }

    touch_thread_best_effort(handle, ctx.thread_id, ctx.thread_uuid).await;
    Ok((
        StatusCode::CREATED,
        Json(ThreadDeliveriesResponse {
            thread_uuid: ctx.thread_uuid,
            subject: ctx.subject.clone(),
            deliveries,
            failed,
            post_message_uuid: None,
        }),
    ))
}

enum RecipientOutcome {
    Delivered(ThreadDeliveryDto),
    Failed(FailedDeliveryDto),
}

async fn deliver_one_recipient(
    handle: &PersistHandle,
    state: &BaseState,
    ctx: &ReplyContext,
    to_uuid: uuid::Uuid,
    body: &PostThreadMessageBody,
) -> RecipientOutcome {
    let to_session_id = match resolve_recipient_id_for_reply(handle, to_uuid).await {
        Ok(id) => id,
        Err(failed) => return RecipientOutcome::Failed(failed),
    };
    let (message_uuid, queued_at) =
        match insert_reply_message(handle, ctx, to_session_id, to_uuid, body).await {
            Ok(v) => v,
            Err(failed) => return RecipientOutcome::Failed(failed),
        };
    let meta = ReplyDeliveryMeta {
        message_uuid,
        queued_at,
    };
    enqueue_reply_reminder(state, ctx, to_uuid, meta, body).await;
    RecipientOutcome::Delivered(ThreadDeliveryDto {
        message_uuid,
        to_session_uuid: to_uuid,
    })
}

async fn resolve_recipient_id_for_reply(
    handle: &PersistHandle,
    to_uuid: uuid::Uuid,
) -> Result<i64, FailedDeliveryDto> {
    match handle.launch().resolve_session_core(to_uuid).await {
        Ok(Some((id, _))) => Ok(id),
        Ok(None) => Err(FailedDeliveryDto {
            to_session_uuid: to_uuid,
            reason: "session_not_found",
        }),
        Err(e) => {
            tracing::warn!(error = %e, %to_uuid, "post_thread_message: recipient lookup failed");
            Err(FailedDeliveryDto {
                to_session_uuid: to_uuid,
                reason: "database_error",
            })
        }
    }
}

async fn insert_reply_message(
    handle: &PersistHandle,
    ctx: &ReplyContext,
    to_session_id: i64,
    to_uuid: uuid::Uuid,
    body: &PostThreadMessageBody,
) -> Result<(uuid::Uuid, chrono::DateTime<chrono::Utc>), FailedDeliveryDto> {
    let message_uuid = uuid::Uuid::new_v4();
    match handle
        .launch()
        .insert_threaded_message_with_context(
            crate::store::persistence::mailbox::ThreadedMessageInsert {
                message_uuid,
                to_session_id: Some(to_session_id),
                from_session_id: ctx.from_session_id,
                body: &body.body,
                thread_id: Some(ctx.thread_id),
                in_reply_to_id: ctx.in_reply_to_id,
            },
            &body.context,
        )
        .await
    {
        Ok(Some(queued_at)) => Ok((message_uuid, queued_at)),
        Ok(None) => Err(FailedDeliveryDto {
            to_session_uuid: to_uuid,
            reason: "recipient_closed",
        }),
        Err(e) => {
            tracing::warn!(error = %e, %to_uuid, "post_thread_message: insert failed");
            Err(FailedDeliveryDto {
                to_session_uuid: to_uuid,
                reason: "insert_failed",
            })
        }
    }
}

struct ReplyDeliveryMeta {
    message_uuid: uuid::Uuid,
    queued_at: chrono::DateTime<chrono::Utc>,
}

async fn enqueue_reply_reminder(
    state: &BaseState,
    ctx: &ReplyContext,
    to_uuid: uuid::Uuid,
    meta: ReplyDeliveryMeta,
    body: &PostThreadMessageBody,
) {
    let from = ctx.from_session_uuid.map(|u| u.to_string());
    let reminder = crate::store::models::SystemReminder {
        payload: crate::store::persistence::mailbox::MailboxEnvelope {
            from_session_uuid: from.as_deref(),
            queued_at: &meta.queued_at,
            body: &body.body,
            to_session_uuid: to_uuid,
            message_uuid: meta.message_uuid,
            thread: Some((&ctx.subject, ctx.thread_uuid)),
        }
        .render(),
        source: crate::store::models::ReminderSource::SessionMailbox {
            message_uuid: meta.message_uuid,
            from_session_uuid: from,
        },
    };
    state
        .mail_push()
        .deliver_or_enqueue(to_uuid, reminder)
        .await;
}

async fn touch_thread_best_effort(handle: &PersistHandle, thread_id: i64, thread_uuid: uuid::Uuid) {
    if let Err(e) = handle.launch().touch_thread(thread_id).await {
        tracing::warn!(error = %e, %thread_uuid, "post_thread_message: touch_thread failed");
    }
}
