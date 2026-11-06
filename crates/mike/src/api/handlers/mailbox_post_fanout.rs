use super::*;

#[path = "mailbox_fanout_delivery.rs"]
mod delivery;

use delivery::{FanoutDeliveryArgs, deliver_fanout_messages};

pub(crate) enum FanoutAudience<'a> {
    Role(&'a str),
    Fleet,
}

impl FanoutAudience<'_> {
    fn label(&self) -> &str {
        match self {
            Self::Role(role) => role.trim(),
            Self::Fleet => "fleet",
        }
    }

    async fn recipients(
        &self,
        handle: &crate::store::persistence::PersistHandle,
        sender_project_id: i64,
        from_session_uuid: uuid::Uuid,
    ) -> Result<Vec<uuid::Uuid>, Response> {
        match self {
            Self::Role(_) => {
                resolve_role_recipients_or_404(
                    handle,
                    sender_project_id,
                    self.label(),
                    from_session_uuid,
                )
                .await
            }
            Self::Fleet => fleet_recipients(handle, from_session_uuid).await,
        }
    }
}

pub(crate) async fn post_session_message_role_fanout(
    state: &AppState,
    handle: &crate::store::persistence::PersistHandle,
    role: &str,
    body: &PostSessionMessageBody,
) -> Result<Response, Response> {
    post_session_message_fanout(state, handle, FanoutAudience::Role(role), body).await
}

async fn fleet_recipients(
    handle: &crate::store::persistence::PersistHandle,
    from_session_uuid: uuid::Uuid,
) -> Result<Vec<uuid::Uuid>, Response> {
    match handle
        .resolve_fleet_target(from_session_uuid, "fleet")
        .await
    {
        Ok(Some(crate::store::persistence::mailbox::FleetTarget::Seats(seats))) => Ok(seats),
        Ok(Some(crate::store::persistence::mailbox::FleetTarget::Seat { session, .. })) => {
            Ok(vec![session])
        }
        Ok(None) => Err(mailbox_error(
            StatusCode::BAD_REQUEST,
            "invalid_fleet_target",
            "`fleet` did not resolve to any seat".to_string(),
        )),
        Err(e) => Err(fleet_target_error_response(e)),
    }
}

async fn attach_requested_tags(
    handle: &crate::store::persistence::PersistHandle,
    thread_id: i64,
    thread_uuid: uuid::Uuid,
    body: &PostSessionMessageBody,
) {
    let Some(tags) = body.tags.as_deref().filter(|t| !t.is_empty()) else {
        return;
    };
    if let Err(e) = handle.attach_tags_to_thread(thread_id, tags).await {
        tracing::warn!(error = %e, %thread_uuid, "post_session_message_role_fanout: tag attach failed");
    }
}

pub(crate) async fn post_session_message_fanout(
    state: &AppState,
    handle: &crate::store::persistence::PersistHandle,
    audience: FanoutAudience<'_>,
    body: &PostSessionMessageBody,
) -> Result<Response, Response> {
    let role = audience.label();
    validate_fanout_prelude(role, &body.message).map_err(|e| *e)?;
    let (from_session_uuid, created_by_session_id, sender_project_id) =
        resolve_fanout_sender(handle, body.from_session_id.as_deref()).await?;
    let threading = parse_fanout_threading_params(body).map_err(|e| *e)?;
    validate_context_and_tags(body).map_err(|e| *e)?;

    let recipient_uuids = audience
        .recipients(handle, sender_project_id, from_session_uuid)
        .await?;
    let sender_label = handle.sender_label(from_session_uuid).await;

    let (thread_id, thread_uuid, subject) = resolve_fanout_thread(
        handle,
        &threading,
        sender_project_id,
        created_by_session_id,
        body,
        role,
    )
    .await
    .map_err(|e| *e)?;

    attach_requested_tags(handle, thread_id, thread_uuid, body).await;

    let (deliveries, failed) = deliver_fanout_messages(
        handle,
        state,
        FanoutDeliveryArgs {
            recipient_uuids: &recipient_uuids,
            thread_id,
            thread_uuid,
            subject: &subject,
            from_session_uuid,
            created_by_session_id,
            body,
            sender_label: sender_label.as_deref(),
        },
    )
    .await;

    let nothing_delivered = deliveries.is_empty();
    let status = if nothing_delivered {
        if failed.iter().all(|item| item.reason == "recipient_closed") {
            StatusCode::CONFLICT
        } else {
            StatusCode::BAD_GATEWAY
        }
    } else {
        StatusCode::CREATED
    };
    let since_you_last_looked = tail_for_sender(state, handle, Some(from_session_uuid)).await;
    let response = (
        status,
        Json(crate::api::changes::WithTail {
            report: ThreadDeliveriesResponse {
                thread_uuid,
                subject,
                deliveries,
                failed,
                post_message_uuid: None,
            },
            since_you_last_looked,
        }),
    )
        .into_response();
    if nothing_delivered {
        Err(response)
    } else {
        Ok(response)
    }
}

fn validate_fanout_prelude(role: &str, message: &str) -> Result<(), Box<Response>> {
    if role.is_empty() {
        return Err(Box::new(mailbox_error(
            StatusCode::BAD_REQUEST,
            "invalid_role",
            "role: addressing requires a non-empty role after the prefix".to_string(),
        )));
    }
    if message.trim().is_empty() {
        return Err(Box::new(mailbox_error(
            StatusCode::BAD_REQUEST,
            "empty_message",
            "message must not be empty".to_string(),
        )));
    }
    Ok(())
}

async fn resolve_fanout_sender(
    handle: &crate::store::persistence::PersistHandle,
    from_raw: Option<&str>,
) -> Result<(uuid::Uuid, i64, i64), Response> {
    let Some(from_raw) = from_raw else {
        return Err(mailbox_error(
            StatusCode::BAD_REQUEST,
            "from_session_id_required",
            "role: addressing requires from_session_id to resolve the project scope to fan out within".to_string(),
        ));
    };
    let from_session_uuid = uuid::Uuid::parse_str(from_raw).map_err(|_| {
        mailbox_error(
            StatusCode::BAD_REQUEST,
            "invalid_from_session_id",
            format!("from_session_id is not a valid UUID: {from_raw}"),
        )
    })?;
    match handle.resolve_session_core(from_session_uuid).await {
        Ok(Some((created_by_session_id, sender_project_id))) => {
            Ok((from_session_uuid, created_by_session_id, sender_project_id))
        }
        Ok(None) => Err(mailbox_error(
            StatusCode::NOT_FOUND,
            "from_session_not_found",
            format!("no session with uuid {from_session_uuid}"),
        )),
        Err(e) => {
            tracing::warn!(error = %e, %from_session_uuid, "post_session_message_role_fanout: sender lookup failed");
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to resolve sender session".to_string(),
            ))
        }
    }
}

struct FanoutThreadingParams {
    thread_id_param: Option<uuid::Uuid>,
    in_reply_to_param: Option<uuid::Uuid>,
    subject_param: Option<String>,
}

fn parse_fanout_threading_params(
    body: &PostSessionMessageBody,
) -> Result<FanoutThreadingParams, Box<Response>> {
    let thread_id_param = match body.thread_id.as_deref() {
        Some(raw) => Some(uuid::Uuid::parse_str(raw).map_err(|_| {
            Box::new(mailbox_error(
                StatusCode::BAD_REQUEST,
                "invalid_thread_id",
                format!("thread_id is not a valid UUID: {raw}"),
            ))
        })?),
        None => None,
    };
    let in_reply_to_param = match body.in_reply_to.as_deref() {
        Some(raw) => Some(uuid::Uuid::parse_str(raw).map_err(|_| {
            Box::new(mailbox_error(
                StatusCode::BAD_REQUEST,
                "invalid_in_reply_to",
                format!("in_reply_to is not a valid UUID: {raw}"),
            ))
        })?),
        None => None,
    };
    let subject_param = body
        .subject
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    if thread_id_param.is_none() && in_reply_to_param.is_none() && subject_param.is_none() {
        return Err(Box::new(mailbox_error(
            StatusCode::BAD_REQUEST,
            "subject_required",
            "multi-recipient sends need thread_id (append) or subject (create) to resolve a thread"
                .to_string(),
        )));
    }
    Ok(FanoutThreadingParams {
        thread_id_param,
        in_reply_to_param,
        subject_param,
    })
}

fn validate_context_and_tags(body: &PostSessionMessageBody) -> Result<(), Box<Response>> {
    if let Some((code, message)) = invalid_context_items(&body.context) {
        return Err(Box::new(mailbox_error(
            StatusCode::BAD_REQUEST,
            code,
            message,
        )));
    }
    if let Some(tags) = body.tags.as_deref()
        && let Some((code, message)) = invalid_tags(tags)
    {
        return Err(Box::new(mailbox_error(
            StatusCode::BAD_REQUEST,
            code,
            message,
        )));
    }
    Ok(())
}

async fn resolve_role_recipients_or_404(
    handle: &crate::store::persistence::PersistHandle,
    sender_project_id: i64,
    role: &str,
    from_session_uuid: uuid::Uuid,
) -> Result<Vec<uuid::Uuid>, Response> {
    let recipient_uuids = match handle
        .resolve_role_recipients(sender_project_id, role, from_session_uuid)
        .await
    {
        Ok(uuids) => uuids,
        Err(e) => {
            tracing::warn!(error = %e, role, "post_session_message_role_fanout: role resolution failed");
            return Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to resolve role recipients".to_string(),
            ));
        }
    };
    if recipient_uuids.is_empty() {
        return Err(mailbox_error(
            StatusCode::NOT_FOUND,
            "no_live_sessions_for_role",
            format!("no live session in this project declares role `{role}`"),
        ));
    }
    Ok(recipient_uuids)
}

async fn resolve_fanout_thread(
    handle: &crate::store::persistence::PersistHandle,
    threading: &FanoutThreadingParams,
    sender_project_id: i64,
    created_by_session_id: i64,
    body: &PostSessionMessageBody,
    role: &str,
) -> Result<(i64, uuid::Uuid, String), Box<Response>> {
    match handle
        .resolve_thread_for_send(
            threading.thread_id_param,
            threading.in_reply_to_param,
            threading.subject_param.as_deref(),
            sender_project_id,
            Some(created_by_session_id),
        )
        .await
    {
        Ok(resolution) => map_fanout_thread_resolution(resolution, body, role),
        Err(e) => {
            tracing::warn!(error = %e, role, "post_session_message_role_fanout: thread resolution failed");
            Err(Box::new(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to resolve thread".to_string(),
            )))
        }
    }
}

fn map_fanout_thread_resolution(
    resolution: crate::store::persistence::mailbox::ThreadResolution,
    body: &PostSessionMessageBody,
    role: &str,
) -> Result<(i64, uuid::Uuid, String), Box<Response>> {
    match resolution {
        crate::store::persistence::mailbox::ThreadResolution::Resolved {
            thread_id,
            thread_uuid,
            subject,
            ..
        } => Ok((thread_id, thread_uuid, subject)),
        crate::store::persistence::mailbox::ThreadResolution::ThreadNotFound => {
            Err(Box::new(mailbox_error(
                StatusCode::NOT_FOUND,
                "thread_not_found",
                format!(
                    "no thread with uuid {}",
                    body.thread_id.as_deref().unwrap_or_default()
                ),
            )))
        }
        crate::store::persistence::mailbox::ThreadResolution::ParentMessageNotFound => {
            Err(Box::new(mailbox_error(
                StatusCode::NOT_FOUND,
                "parent_message_not_found",
                format!(
                    "no message with uuid {}",
                    body.in_reply_to.as_deref().unwrap_or_default()
                ),
            )))
        }
        crate::store::persistence::mailbox::ThreadResolution::None => {
            tracing::error!(
                role,
                "post_session_message_role_fanout: resolve_thread_for_send returned None despite a threading param"
            );
            Err(Box::new(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "internal error resolving thread".to_string(),
            )))
        }
    }
}
