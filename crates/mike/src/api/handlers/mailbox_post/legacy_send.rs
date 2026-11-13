use crate::store::persistence::launch_repo::LaunchPersist as _;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::api::handlers::{
    PostSessionMessageResponse, fetch_mailbox_delivery_fields, mailbox_error, tail_for_sender,
};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;

use super::{SendContext, record_failed_send};

pub(super) async fn send(
    handle: &PersistHandle,
    state: &AppState,
    ctx: SendContext<'_>,
) -> Result<Response, Response> {
    match handle
        .launch()
        .insert_message_with_context(
            ctx.message_uuid,
            ctx.to_session_uuid,
            ctx.from_session_uuid,
            &ctx.body.message,
            &ctx.body.context,
        )
        .await
    {
        Ok(Some(inserted)) => Ok(queued_response(
            handle,
            state,
            &ctx,
            inserted.message_uuid,
            inserted.from_session_uuid,
            inserted.queued_at,
        )
        .await),
        Ok(None) => Err(unqueued(handle, &ctx).await),
        Err(crate::store::persistence::PersistError::DataInvariant(msg)) => {
            record_failed_send(handle, None, ctx.to_session_uuid, "from_session_not_found").await;
            Err(mailbox_error(
                StatusCode::NOT_FOUND,
                "from_session_not_found",
                msg,
            ))
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                to_session_uuid = %ctx.to_session_uuid,
                "post_session_message: DB insert failed"
            );
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

async fn unqueued(handle: &PersistHandle, ctx: &SendContext<'_>) -> Response {
    if let Err(response) =
        super::refuse_closed_fleet_seat(handle, ctx.from_session_uuid, ctx.to_session_uuid, false)
            .await
    {
        return response;
    }
    let exists = match handle
        .launch()
        .resolve_session_core(ctx.to_session_uuid)
        .await
    {
        Ok(core) => core.is_some(),
        Err(err) => {
            tracing::warn!(error = %err, to = %ctx.to_session_uuid, "mailbox: could not classify an unqueued delivery");
            return mailbox_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "recipient_state_unavailable",
                "the message was not queued and the recipient's current state could not be read"
                    .into(),
            );
        }
    };
    if !exists {
        record_failed_send(
            handle,
            ctx.from_session_uuid,
            ctx.to_session_uuid,
            "session_not_found",
        )
        .await;
    }
    let (status, code, message) = unqueued_error(ctx.to_session_uuid, exists);
    mailbox_error(status, code, message)
}

fn unqueued_error(to: uuid::Uuid, exists: bool) -> (StatusCode, &'static str, String) {
    if exists {
        (
            StatusCode::CONFLICT,
            "recipient_lifecycle_changed",
            format!(
                "session {to} changed lifecycle while delivery was refused; the message was not queued. Resolve the recipient's current session before retrying"
            ),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            "session_not_found",
            format!(
                "no session with uuid {to} — pass the exact UUID from get_workspace_roster; do not pad short ids with zeros"
            ),
        )
    }
}

async fn queued_response(
    handle: &PersistHandle,
    state: &AppState,
    ctx: &SendContext<'_>,
    message_uuid: uuid::Uuid,
    from_session_uuid: Option<uuid::Uuid>,
    queued_at: chrono::DateTime<chrono::Utc>,
) -> Response {
    let from = from_session_uuid.map(|u| u.to_string());
    let sender_label = match from_session_uuid {
        Some(u) => handle.launch().sender_label(u).await,
        None => None,
    };
    let reminder = crate::store::models::SystemReminder {
        payload: crate::store::persistence::mailbox::MailboxEnvelope {
            from_session_uuid: from.as_deref(),
            queued_at: &queued_at,
            body: &ctx.body.message,
            to_session_uuid: ctx.to_session_uuid,
            message_uuid,
            thread: None,
        }
        .render_as(sender_label.as_deref()),
        source: crate::store::models::ReminderSource::SessionMailbox {
            message_uuid,
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

    let since_you_last_looked = tail_for_sender(state, handle, from_session_uuid).await;
    (
        StatusCode::CREATED,
        Json(crate::api::changes::WithTail {
            report: PostSessionMessageResponse {
                message_uuid,
                to_session_uuid: ctx.to_session_uuid,
                from_session_uuid,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopened_existing_recipient_retains_refusal_without_missing_session_claim() {
        let (status, code, message) = unqueued_error(uuid::Uuid::new_v4(), true);
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(code, "recipient_lifecycle_changed");
        assert!(message.contains("not queued"));
        assert!(!message.contains("no session"));
    }

    #[test]
    fn absent_recipient_remains_a_missing_session() {
        let (status, code, _) = unqueued_error(uuid::Uuid::new_v4(), false);
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(code, "session_not_found");
    }
}
