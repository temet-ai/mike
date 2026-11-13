use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::api::handlers::{
    PostSessionMessageResponse, fetch_mailbox_delivery_fields, mailbox_error, tail_for_sender,
};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::mailbox::ThreadResolution;

use super::{SendContext, record_failed_send};

pub(super) struct ThreadingParams<'a> {
    pub(super) thread_id_param: Option<uuid::Uuid>,
    pub(super) in_reply_to_param: Option<uuid::Uuid>,
    pub(super) subject_param: Option<&'a str>,
    pub(super) raw_thread_id: Option<&'a str>,
    pub(super) raw_in_reply_to: Option<&'a str>,
}

pub(super) async fn send(
    handle: &PersistHandle,
    state: &AppState,
    ctx: SendContext<'_>,
    threading: ThreadingParams<'_>,
) -> Result<Response, Response> {
    let (to_session_id, to_project_id) = resolve_recipient_core(handle, &ctx).await?;
    let created_by_session_id = resolve_sender_id(handle, &ctx).await?;

    let (thread_id, thread_uuid, subject, in_reply_to_id) = resolve_thread(
        handle,
        &ctx,
        &threading,
        to_project_id,
        created_by_session_id,
    )
    .await?;
    let queued_at = insert_threaded_message_or_fail(
        handle,
        &ctx,
        to_session_id,
        thread_id,
        created_by_session_id,
        in_reply_to_id,
    )
    .await?;

    if let Some(tags) = ctx.body.tags.as_deref().filter(|t| !t.is_empty())
        && let Err(e) = handle.launch().attach_tags_to_thread(thread_id, tags).await
    {
        tracing::warn!(error = %e, %thread_uuid, "post_session_message: tag attach failed");
    }

    Ok(queued_response(handle, state, &ctx, queued_at, &subject, thread_uuid).await)
}

async fn insert_threaded_message_or_fail(
    handle: &PersistHandle,
    ctx: &SendContext<'_>,
    to_session_id: i64,
    thread_id: i64,
    created_by_session_id: Option<i64>,
    in_reply_to_id: Option<i64>,
) -> Result<chrono::DateTime<chrono::Utc>, Response> {
    match handle
        .launch()
        .insert_threaded_message_with_context(
            crate::store::persistence::mailbox::ThreadedMessageInsert {
                message_uuid: ctx.message_uuid,
                to_session_id: Some(to_session_id),
                from_session_id: created_by_session_id,
                body: &ctx.body.message,
                thread_id: Some(thread_id),
                in_reply_to_id,
            },
            &ctx.body.context,
        )
        .await
    {
        Ok(Some(t)) => Ok(t),
        Ok(None) => Err(mailbox_error(
            StatusCode::CONFLICT,
            "recipient_closed",
            format!(
                "session {} closed before the message was queued; resolve its successor and retry",
                ctx.to_session_uuid
            ),
        )),
        Err(e) => {
            tracing::warn!(error = %e, to_session_uuid = %ctx.to_session_uuid, "post_session_message: threaded insert failed");
            record_failed_send(
                handle,
                ctx.from_session_uuid,
                ctx.to_session_uuid,
                "database_error",
            )
            .await;
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to queue mailbox message".to_string(),
            ))
        }
    }
}

async fn resolve_recipient_core(
    handle: &PersistHandle,
    ctx: &SendContext<'_>,
) -> Result<(i64, i64), Response> {
    let Some(core) = (match handle
        .launch()
        .resolve_session_core(ctx.to_session_uuid)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, to_session_uuid = %ctx.to_session_uuid, "post_session_message: recipient lookup failed");
            return Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to resolve recipient session".to_string(),
            ));
        }
    }) else {
        record_failed_send(
            handle,
            ctx.from_session_uuid,
            ctx.to_session_uuid,
            "session_not_found",
        )
        .await;
        return Err(mailbox_error(
            StatusCode::NOT_FOUND,
            "session_not_found",
            format!(
                "no session with uuid {} — pass the exact UUID from \
                 get_workspace_roster; do not pad short ids with zeros",
                ctx.to_session_uuid
            ),
        ));
    };
    Ok(core)
}

async fn resolve_sender_id(
    handle: &PersistHandle,
    ctx: &SendContext<'_>,
) -> Result<Option<i64>, Response> {
    let Some(u) = ctx.from_session_uuid else {
        return Ok(None);
    };
    match handle.launch().resolve_session_core(u).await {
        Ok(Some((id, _))) => Ok(Some(id)),
        Ok(None) => {
            record_failed_send(handle, None, ctx.to_session_uuid, "from_session_not_found").await;
            Err(mailbox_error(
                StatusCode::NOT_FOUND,
                "from_session_not_found",
                format!("no session with uuid {u}"),
            ))
        }
        Err(e) => {
            tracing::warn!(error = %e, %u, "post_session_message: sender lookup failed");
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to resolve sender session".to_string(),
            ))
        }
    }
}

async fn resolve_thread(
    handle: &PersistHandle,
    ctx: &SendContext<'_>,
    threading: &ThreadingParams<'_>,
    to_project_id: i64,
    created_by_session_id: Option<i64>,
) -> Result<(i64, uuid::Uuid, String, Option<i64>), Response> {
    match handle
        .launch()
        .resolve_thread_for_send(
            threading.thread_id_param,
            threading.in_reply_to_param,
            threading.subject_param,
            to_project_id,
            created_by_session_id,
        )
        .await
    {
        Ok(resolution) => map_thread_resolution(handle, ctx, threading, resolution).await,
        Err(e) => {
            tracing::warn!(error = %e, to_session_uuid = %ctx.to_session_uuid, "post_session_message: thread resolution failed");
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to resolve thread".to_string(),
            ))
        }
    }
}

async fn map_thread_resolution(
    handle: &PersistHandle,
    ctx: &SendContext<'_>,
    threading: &ThreadingParams<'_>,
    resolution: ThreadResolution,
) -> Result<(i64, uuid::Uuid, String, Option<i64>), Response> {
    match resolution {
        ThreadResolution::Resolved {
            thread_id,
            thread_uuid,
            subject,
            in_reply_to_id,
        } => Ok((thread_id, thread_uuid, subject, in_reply_to_id)),
        ThreadResolution::ThreadNotFound => {
            record_failed_send(
                handle,
                ctx.from_session_uuid,
                ctx.to_session_uuid,
                "thread_not_found",
            )
            .await;
            Err(mailbox_error(
                StatusCode::NOT_FOUND,
                "thread_not_found",
                format!(
                    "no thread with uuid {}",
                    threading.raw_thread_id.unwrap_or_default()
                ),
            ))
        }
        ThreadResolution::ParentMessageNotFound => {
            record_failed_send(
                handle,
                ctx.from_session_uuid,
                ctx.to_session_uuid,
                "parent_message_not_found",
            )
            .await;
            Err(mailbox_error(
                StatusCode::NOT_FOUND,
                "parent_message_not_found",
                format!(
                    "no message with uuid {}",
                    threading.raw_in_reply_to.unwrap_or_default()
                ),
            ))
        }
        ThreadResolution::None => {
            tracing::error!(
                to_session_uuid = %ctx.to_session_uuid,
                "post_session_message: resolve_thread_for_send returned None despite a threading param"
            );
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "internal error resolving thread".to_string(),
            ))
        }
    }
}

async fn queued_response(
    handle: &PersistHandle,
    state: &AppState,
    ctx: &SendContext<'_>,
    queued_at: chrono::DateTime<chrono::Utc>,
    subject: &str,
    thread_uuid: uuid::Uuid,
) -> Response {
    let from = ctx.from_session_uuid.map(|u| u.to_string());
    let sender_label = match ctx.from_session_uuid {
        Some(u) => handle.fleet().sender_label(u).await,
        None => None,
    };
    let reminder = crate::store::models::SystemReminder {
        payload: crate::store::persistence::mailbox::MailboxEnvelope {
            from_session_uuid: from.as_deref(),
            queued_at: &queued_at,
            body: &ctx.body.message,
            to_session_uuid: ctx.to_session_uuid,
            message_uuid: ctx.message_uuid,
            thread: Some((subject, thread_uuid)),
        }
        .render_as(sender_label.as_deref()),
        source: crate::store::models::ReminderSource::SessionMailbox {
            message_uuid: ctx.message_uuid,
            from_session_uuid: from,
        },
    };
    let pushed = crate::proxy::remote_control::inject::deliver_or_enqueue(
        &state.push_deps(),
        ctx.to_session_uuid,
        reminder,
    )
    .await;

    let (recipient_last_request_at, recipient_last_eligible_turn_at, activity_state) =
        fetch_mailbox_delivery_fields(handle, state, ctx.to_session_uuid).await;
    let delivery_state = crate::api::handlers::mailbox_reads::delivery_state_after_send(
        pushed,
        crate::mcp_channel::listeners().is_live(ctx.to_session_uuid),
        activity_state,
    );

    let since_you_last_looked = tail_for_sender(state, handle, ctx.from_session_uuid).await;
    (
        StatusCode::CREATED,
        Json(crate::api::changes::WithTail {
            report: PostSessionMessageResponse {
                message_uuid: ctx.message_uuid,
                to_session_uuid: ctx.to_session_uuid,
                from_session_uuid: ctx.from_session_uuid,
                queued_at: queued_at.to_rfc3339(),
                recipient_liveness: ctx.liveness.map(crate::workspace::Liveness::as_str),
                recipient_last_request_at,
                recipient_last_eligible_turn_at,
                delivery_state,
            },
            since_you_last_looked,
        }),
    )
        .into_response()
}
