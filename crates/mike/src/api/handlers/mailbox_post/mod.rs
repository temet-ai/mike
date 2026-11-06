use super::*;

mod audit;
mod legacy_send;
mod threaded_send;
mod validation;

use audit::record_failed_send;
use validation::validate_send_request;

struct SendContext<'a> {
    to_session_uuid: uuid::Uuid,
    from_session_uuid: Option<uuid::Uuid>,
    message_uuid: uuid::Uuid,
    liveness: Option<crate::workspace::Liveness>,
    body: &'a PostSessionMessageBody,
}

pub(crate) async fn post_session_message(
    State(state): State<AppState>,
    Path(to_session_id): Path<String>,
    Json(body): Json<PostSessionMessageBody>,
) -> Result<Response, Response> {
    let Some(handle) = state.persist() else {
        return Err(mailbox_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistence_disabled",
            "session persistence is disabled; the mailbox is not available".to_string(),
        ));
    };

    if let Some(role) = to_session_id.strip_prefix("role:") {
        return post_session_message_role_fanout(&state, handle, role, &body).await;
    }
    if to_session_id.trim() == "fleet" {
        return post_session_message_fanout(&state, handle, FanoutAudience::Fleet, &body).await;
    }

    let to_session_uuid = if let Some(name) = to_session_id.strip_prefix("name:") {
        resolve_name_addressed_recipient(handle, name, &body).await?
    } else if is_fleet_form(&to_session_id) {
        resolve_fleet_addressed_recipient(handle, &to_session_id, &body).await?
    } else {
        resolve_mailbox_recipient(handle, &to_session_id).await?
    };

    let parsed = validate_send_request(handle, to_session_uuid, &body).await?;

    refuse_closed_fleet_seat(handle, parsed.from_session_uuid, to_session_uuid, true).await?;

    let liveness = liveness_for_gate(handle, to_session_uuid).await;
    if state.config().mailbox_reject_send_to_ended_session
        && liveness == Some(crate::workspace::Liveness::Ended)
    {
        record_failed_send(
            handle,
            parsed.from_session_uuid,
            to_session_uuid,
            "recipient_ended",
        )
        .await;
        return Err(mailbox_error(
            StatusCode::CONFLICT,
            "recipient_ended",
            format!(
                "session {to_session_uuid} has ended its workspace registration — it will never turn again to read this message"
            ),
        ));
    }

    let ctx = SendContext {
        to_session_uuid,
        from_session_uuid: parsed.from_session_uuid,
        message_uuid: uuid::Uuid::new_v4(),
        liveness,
        body: &body,
    };

    let facts = mail_facts(&ctx);
    let sent = if parsed.threading_requested {
        threaded_send::send(
            handle,
            &state,
            ctx,
            threaded_send::ThreadingParams {
                thread_id_param: parsed.thread_id_param,
                in_reply_to_param: parsed.in_reply_to_param,
                subject_param: parsed.subject_param.as_deref(),
                raw_thread_id: body.thread_id.as_deref(),
                raw_in_reply_to: body.in_reply_to.as_deref(),
            },
        )
        .await
    } else {
        legacy_send::send(handle, &state, ctx).await
    };
    if sent.is_ok() {
        crate::change_record::writers::spawn_mail_event(
            &state.tracker(),
            handle.clone(),
            state.feature_toggle_overlay.load().change_record,
            facts,
        );
    }
    sent
}

async fn refuse_closed_fleet_seat(
    handle: &crate::store::persistence::PersistHandle,
    from: Option<uuid::Uuid>,
    to: uuid::Uuid,
    audit: bool,
) -> Result<(), Response> {
    let closed = match handle.closed_fleet_seat(to).await {
        Ok(closed) => closed,
        Err(e) => {
            tracing::warn!(error = %e, %to, "post_session_message: closed-seat lookup failed");
            return Err(mailbox_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "recipient_state_unavailable",
                "could not verify whether the recipient seat is open; retry the send".to_owned(),
            ));
        }
    };
    let Some(closed) = closed else {
        return Ok(());
    };
    if audit {
        record_failed_send(handle, from, to, "recipient_ended").await;
    }
    Err(mailbox_error(
        StatusCode::CONFLICT,
        "recipient_closed",
        closed.refusal(to),
    ))
}

fn mail_facts(ctx: &SendContext<'_>) -> crate::change_record::writers::MailFacts {
    crate::change_record::writers::MailFacts {
        message_uuid: ctx.message_uuid,
        from: ctx.from_session_uuid,
        to: ctx.to_session_uuid,
        body: ctx.body.message.clone(),
        context: ctx.body.context.clone(),
        at: chrono::Utc::now(),
    }
}

async fn liveness_for_gate(
    handle: &crate::store::persistence::PersistHandle,
    to_session_uuid: uuid::Uuid,
) -> Option<crate::workspace::Liveness> {
    match handle.fetch_session_liveness(to_session_uuid).await {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!(
                error = %e,
                %to_session_uuid,
                "post_session_message: liveness lookup failed — proceeding without a liveness gate"
            );
            None
        }
    }
}
