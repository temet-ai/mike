use super::*;

pub(super) struct FanoutDeliveryArgs<'a> {
    pub(super) recipient_uuids: &'a [uuid::Uuid],
    pub(super) thread_id: i64,
    pub(super) thread_uuid: uuid::Uuid,
    pub(super) subject: &'a str,
    pub(super) from_session_uuid: uuid::Uuid,
    pub(super) created_by_session_id: i64,
    pub(super) body: &'a PostSessionMessageBody,
    pub(super) sender_label: Option<&'a str>,
}

pub(super) async fn deliver_fanout_messages(
    handle: &crate::store::persistence::PersistHandle,
    state: &AppState,
    args: FanoutDeliveryArgs<'_>,
) -> (Vec<ThreadDeliveryDto>, Vec<FailedDeliveryDto>) {
    let mut deliveries = Vec::with_capacity(args.recipient_uuids.len());
    let mut failed = Vec::new();
    for to_uuid in args.recipient_uuids {
        match deliver_one_fanout_recipient(handle, state, &args, *to_uuid).await {
            Ok(dto) => deliveries.push(dto),
            Err(dto) => failed.push(dto),
        }
    }
    (deliveries, failed)
}

async fn deliver_one_fanout_recipient(
    handle: &crate::store::persistence::PersistHandle,
    state: &AppState,
    args: &FanoutDeliveryArgs<'_>,
    to_uuid: uuid::Uuid,
) -> Result<ThreadDeliveryDto, FailedDeliveryDto> {
    let to_session_id = resolve_fanout_recipient_id(handle, to_uuid).await?;
    let (message_uuid, queued_at) =
        insert_fanout_message(handle, args, to_session_id, to_uuid).await?;
    let reminder = crate::store::models::SystemReminder {
        payload: crate::store::persistence::mailbox::MailboxEnvelope {
            from_session_uuid: Some(&args.from_session_uuid.to_string()),
            queued_at: &queued_at,
            body: &args.body.message,
            to_session_uuid: to_uuid,
            message_uuid,
            thread: Some((args.subject, args.thread_uuid)),
        }
        .render_as(args.sender_label),
        source: crate::store::models::ReminderSource::SessionMailbox {
            message_uuid,
            from_session_uuid: Some(args.from_session_uuid.to_string()),
        },
    };
    crate::proxy::remote_control::inject::deliver_or_enqueue(&state.push_deps(), to_uuid, reminder)
        .await;
    Ok(ThreadDeliveryDto {
        message_uuid,
        to_session_uuid: to_uuid,
    })
}

async fn resolve_fanout_recipient_id(
    handle: &crate::store::persistence::PersistHandle,
    to_uuid: uuid::Uuid,
) -> Result<i64, FailedDeliveryDto> {
    match handle.launch().resolve_session_core(to_uuid).await {
        Ok(Some((id, _))) => Ok(id),
        Ok(None) => Err(failed_delivery(to_uuid, "session_not_found")),
        Err(e) => {
            tracing::warn!(error = %e, %to_uuid, "post_session_message_role_fanout: recipient lookup failed");
            Err(failed_delivery(to_uuid, "database_error"))
        }
    }
}

async fn insert_fanout_message(
    handle: &crate::store::persistence::PersistHandle,
    args: &FanoutDeliveryArgs<'_>,
    to_session_id: i64,
    to_uuid: uuid::Uuid,
) -> Result<(uuid::Uuid, chrono::DateTime<chrono::Utc>), FailedDeliveryDto> {
    let message_uuid = uuid::Uuid::new_v4();
    match handle
        .launch()
        .insert_threaded_message_with_context(
            crate::store::persistence::mailbox::ThreadedMessageInsert {
                message_uuid,
                to_session_id: Some(to_session_id),
                from_session_id: Some(args.created_by_session_id),
                body: &args.body.message,
                thread_id: Some(args.thread_id),
                in_reply_to_id: None,
            },
            &args.body.context,
        )
        .await
    {
        Ok(Some(queued_at)) => Ok((message_uuid, queued_at)),
        Ok(None) => Err(failed_delivery(to_uuid, "recipient_closed")),
        Err(e) => {
            tracing::warn!(error = %e, %to_uuid, "post_session_message_role_fanout: insert failed");
            Err(failed_delivery(to_uuid, "insert_failed"))
        }
    }
}

fn failed_delivery(to_session_uuid: uuid::Uuid, reason: &'static str) -> FailedDeliveryDto {
    FailedDeliveryDto {
        to_session_uuid,
        reason,
    }
}
