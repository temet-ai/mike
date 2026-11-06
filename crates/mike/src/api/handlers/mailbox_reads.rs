use super::*;

pub(crate) async fn fetch_mailbox_delivery_fields(
    handle: &crate::store::persistence::PersistHandle,
    state: &AppState,
    to_session_uuid: uuid::Uuid,
) -> (Option<String>, Option<String>, Option<&'static str>) {
    let activity = match handle.fetch_recipient_activity(to_session_uuid).await {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!(
                error = %e,
                %to_session_uuid,
                "fetch_mailbox_delivery_fields: recipient activity lookup failed"
            );
            None
        }
    };
    match activity {
        Some(a) => {
            let delivery_ttl_minutes = state.config().mailbox_delivery_ttl_minutes;
            let within_ttl = a.last_request_at.is_some_and(|t| {
                chrono::Utc::now() - t <= chrono::Duration::minutes(delivery_ttl_minutes)
            });
            (
                a.last_request_at.map(|t| t.to_rfc3339()),
                a.last_eligible_turn_at.map(|t| t.to_rfc3339()),
                Some(if within_ttl {
                    "awaiting_eligible_turn"
                } else {
                    "recipient_inactive"
                }),
            )
        }
        None => (None, None, None),
    }
}

pub(crate) async fn get_session_messages(
    State(state): State<AppState>,
    Path(session_uuid): Path<uuid::Uuid>,
    headers: HeaderMap,
) -> Response {
    let Some(handle) = state.persist() else {
        return mailbox_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_disabled",
            "session persistence is disabled; the mailbox is not available".to_string(),
        );
    };
    let poll = crate::api::poll_answers::Poll {
        key: format!("session-messages:{session_uuid}"),
        freshness: crate::api::poll_answers::SESSION_MAILBOX,
        scope: vec![session_uuid.to_string()],
        headers: &headers,
    };
    crate::api::poll_answers::answer_json(&state, poll, || {
        load_session_messages(handle, &state, session_uuid)
    })
    .await
}

async fn load_session_messages(
    handle: &crate::store::persistence::PersistHandle,
    state: &AppState,
    session_uuid: uuid::Uuid,
) -> Result<crate::api::poll_answers::Loaded<Bytes>, Response> {
    let rows = handle
        .fetch_for_session_with_activity(session_uuid)
        .await
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                %session_uuid,
                "get_session_messages: DB query failed"
            );
            mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to fetch mailbox messages".to_string(),
            )
        })?;
    let now = chrono::Utc::now();
    let stale_threshold_minutes = state.config().mailbox_stale_minutes;
    let delivery_ttl_minutes = state.config().mailbox_delivery_ttl_minutes;
    let dto_of = |r| {
        mailbox_message_dto_with_activity(r, now, stale_threshold_minutes, delivery_ttl_minutes)
    };
    let dto = SessionMessagesDto {
        session_id: session_uuid,
        inbox: rows.inbox.into_iter().map(dto_of).collect(),
        outbox: rows.outbox.into_iter().map(dto_of).collect(),
    };
    let sessions = dto
        .outbox
        .iter()
        .filter_map(|m| m.to_session_uuid)
        .map(|u| u.to_string())
        .collect();
    Ok(crate::api::poll_answers::Loaded {
        value: crate::api::poll_answers::json_bytes(&dto)?,
        sessions,
    })
}

pub(crate) async fn get_session_message(
    State(state): State<AppState>,
    Path((session_uuid, message_uuid)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> Result<Json<MailboxMessageDto>, Response> {
    let Some(handle) = state.persist() else {
        return Err(mailbox_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_disabled",
            "session persistence is disabled; the mailbox is not available".to_string(),
        ));
    };
    match handle
        .fetch_message_for_recipient(message_uuid, session_uuid)
        .await
    {
        Ok(Some(row)) => {
            let now = chrono::Utc::now();
            let stale_threshold_minutes = state.config().mailbox_stale_minutes;
            Ok(Json(mailbox_message_dto(row, now, stale_threshold_minutes)))
        }
        Ok(None) => Err(mailbox_error(
            StatusCode::NOT_FOUND,
            "message_not_found",
            format!("no mailbox message {message_uuid} addressed to session {session_uuid}"),
        )),
        Err(e) => {
            tracing::warn!(
                error = %e,
                %session_uuid,
                %message_uuid,
                "get_session_message: DB query failed"
            );
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to fetch mailbox message".to_string(),
            ))
        }
    }
}

pub(crate) async fn get_session_message_receipt(
    State(state): State<AppState>,
    Path((session_uuid, message_uuid)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> Result<Json<MailboxMessageDto>, Response> {
    let Some(handle) = state.persist() else {
        return Err(mailbox_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_disabled",
            "session persistence is disabled; the mailbox is not available".to_string(),
        ));
    };
    match handle
        .fetch_message_for_sender(message_uuid, session_uuid)
        .await
    {
        Ok(Some(row)) => {
            let now = chrono::Utc::now();
            let stale_threshold_minutes = state.config().mailbox_stale_minutes;
            Ok(Json(mailbox_message_dto(row, now, stale_threshold_minutes)))
        }
        Ok(None) => Err(mailbox_error(
            StatusCode::NOT_FOUND,
            "message_not_found",
            format!("no mailbox message {message_uuid} sent by session {session_uuid}"),
        )),
        Err(e) => {
            tracing::warn!(
                error = %e,
                %session_uuid,
                %message_uuid,
                "get_session_message_receipt: DB query failed"
            );
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to fetch mailbox message receipt".to_string(),
            ))
        }
    }
}

#[derive(Serialize)]
pub(crate) struct AckMailboxMessageResponse {
    pub(crate) message_uuid: uuid::Uuid,
    pub(crate) consumed_at: String,
}

pub(crate) async fn ack_session_message(
    State(state): State<AppState>,
    Path((session_uuid, message_uuid)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> Result<Json<AckMailboxMessageResponse>, Response> {
    let Some(handle) = state.persist() else {
        return Err(mailbox_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_disabled",
            "session persistence is disabled; the mailbox is not available".to_string(),
        ));
    };
    match handle.ack_message(session_uuid, message_uuid).await {
        Ok(crate::store::persistence::mailbox::AckOutcome::Consumed(consumed_at)) => {
            state
                .store
                .write()
                .await
                .remove_mailbox_reminder(&session_uuid.to_string(), message_uuid);
            Ok(Json(AckMailboxMessageResponse {
                message_uuid,
                consumed_at: consumed_at.to_rfc3339(),
            }))
        }
        Ok(crate::store::persistence::mailbox::AckOutcome::NeverDelivered) => Err(mailbox_error(
            StatusCode::CONFLICT,
            "not_yet_delivered",
            format!(
                "message {message_uuid} for session {session_uuid} exists but was never \
                 delivered — the recipient has not yet seen it in a turn, so this ack was \
                 refused (issue #185: acking undelivered mail let it masquerade as read). \
                 Wait for it to be injected into a turn, then retry the ack. Confirm delivery \
                 status first: curl http://127.0.0.1:9989/api/sessions/{session_uuid}/messages/{message_uuid}"
            ),
        )),
        Ok(crate::store::persistence::mailbox::AckOutcome::NotFound) => Err(mailbox_error(
            StatusCode::NOT_FOUND,
            "message_not_found",
            format!("no message {message_uuid} addressed to session {session_uuid}"),
        )),
        Err(e) => {
            tracing::warn!(
                error = %e,
                %session_uuid,
                %message_uuid,
                "ack_session_message: DB query failed"
            );
            Err(mailbox_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "failed to acknowledge mailbox message".to_string(),
            ))
        }
    }
}

pub(crate) fn delivery_state_after_send(
    pushed: bool,
    by_channel: bool,
    activity_state: Option<&'static str>,
) -> Option<&'static str> {
    if !pushed && by_channel {
        return Some("channel");
    }
    delivery_state_after_push(pushed, activity_state)
}

pub(crate) fn delivery_state_after_push(
    pushed: bool,
    activity_state: Option<&'static str>,
) -> Option<&'static str> {
    if pushed {
        Some("pushed")
    } else {
        activity_state
    }
}

#[cfg(test)]
mod delivery_state_tests {
    use super::{delivery_state_after_push, delivery_state_after_send};

    #[test]
    fn a_pushed_send_reports_pushed_whatever_the_activity_says() {
        assert_eq!(
            delivery_state_after_push(true, Some("awaiting_eligible_turn")),
            Some("pushed")
        );
        assert_eq!(delivery_state_after_push(true, None), Some("pushed"));
        assert_eq!(
            delivery_state_after_send(false, true, Some("awaiting_eligible_turn")),
            Some("channel")
        );
        assert_eq!(delivery_state_after_send(true, true, None), Some("pushed"));
        assert_eq!(
            delivery_state_after_send(false, false, Some("recipient_inactive")),
            Some("recipient_inactive")
        );
    }

    #[test]
    fn an_unpushed_send_keeps_the_activity_state() {
        assert_eq!(
            delivery_state_after_push(false, Some("recipient_inactive")),
            Some("recipient_inactive")
        );
    }
}
