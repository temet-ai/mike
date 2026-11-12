use super::requeue_reminder;
use crate::state::AppState;
use crate::store::models::{ReminderSource, SystemReminder};
use crate::store::persistence::{PersistError, PersistHandle};

pub(in crate::steerer::stage) enum MailboxClaim {
    NotMailbox(SystemReminder),
    Proceed(SystemReminder, bool),
    Skip,
}

pub(in crate::steerer::stage) async fn claim_mailbox_reminder(
    state: &AppState,
    session_id: &str,
    reminder: SystemReminder,
    mailbox_seen: &mut usize,
    mailbox_skip_count: usize,
) -> MailboxClaim {
    let ReminderSource::SessionMailbox { message_uuid, .. } = &reminder.source else {
        return MailboxClaim::NotMailbox(reminder);
    };
    let message_uuid = *message_uuid;
    *mailbox_seen += 1;
    if *mailbox_seen <= mailbox_skip_count {
        requeue_reminder(state, session_id, reminder).await;
        return MailboxClaim::Skip;
    }
    let Some(persist) = &state.persist else {
        log_mailbox_persistence_disabled(session_id, message_uuid);
        return MailboxClaim::Skip;
    };
    if left_to_the_channel(session_id, message_uuid) {
        requeue_reminder(state, session_id, reminder).await;
        return MailboxClaim::Skip;
    }

    let first_claim = match persist.claim_message(message_uuid).await {
        Ok(claimed) => claimed,
        Err(e) => {
            log_mailbox_claim_failed(session_id, message_uuid, &e);
            requeue_reminder(state, session_id, reminder).await;
            return MailboxClaim::Skip;
        }
    };

    if first_claim {
        let redelivery_enabled = state.feature_toggle_overlay.load().mailbox_redelivery;
        let requeue = redelivery_enabled && state.config().mailbox_redelivery_max_attempts > 1;
        return MailboxClaim::Proceed(reminder, requeue);
    }

    resolve_mailbox_redelivery(state, persist, session_id, reminder, message_uuid).await
}

fn left_to_the_channel(session_id: &str, message_uuid: uuid::Uuid) -> bool {
    let channel_is_live = session_id
        .parse::<uuid::Uuid>()
        .is_ok_and(|session| crate::mcp_channel::listeners().is_live(session));
    channel_is_live && crate::mcp_channel::mail_left_to_the_channel().still_has_time(message_uuid)
}

fn log_mailbox_persistence_disabled(session_id: &str, message_uuid: uuid::Uuid) {
    tracing::warn!(
        session_id = %session_id,
        %message_uuid,
        "mailbox reminder with persistence disabled — dropping"
    );
}

fn log_mailbox_claim_failed(session_id: &str, message_uuid: uuid::Uuid, e: &PersistError) {
    tracing::warn!(
        session_id = %session_id,
        %message_uuid,
        error = %e,
        "mailbox claim failed — requeueing"
    );
}

async fn resolve_mailbox_redelivery(
    state: &AppState,
    persist: &PersistHandle,
    session_id: &str,
    reminder: SystemReminder,
    message_uuid: uuid::Uuid,
) -> MailboxClaim {
    if !state.feature_toggle_overlay.load().mailbox_redelivery {
        log_redelivery_disabled(session_id, message_uuid);
        return MailboxClaim::Skip;
    }

    match persist
        .bump_redelivery_attempt(
            message_uuid,
            state.config().mailbox_redelivery_max_attempts,
            state.config().mailbox_redelivery_backoff_base_secs,
            state.config().mailbox_redelivery_backoff_cap_secs,
        )
        .await
    {
        Ok(Some(outcome)) if !outcome.attempted => {
            log_redelivery_backoff_not_elapsed(session_id, message_uuid, outcome.delivery_attempts);
            requeue_reminder(state, session_id, reminder).await;
            MailboxClaim::Skip
        }
        Ok(Some(outcome)) => {
            if outcome.dead_lettered {
                tell_sender_not_delivered(persist, message_uuid).await;
            }
            log_redelivery_bumped(
                session_id,
                message_uuid,
                outcome.delivery_attempts,
                outcome.dead_lettered,
            );
            MailboxClaim::Proceed(reminder, !outcome.dead_lettered)
        }
        Ok(None) => {
            log_redelivery_not_eligible(session_id, message_uuid);
            MailboxClaim::Skip
        }
        Err(e) => {
            log_redelivery_bump_failed(session_id, message_uuid, &e);
            requeue_reminder(state, session_id, reminder).await;
            MailboxClaim::Skip
        }
    }
}

async fn tell_sender_not_delivered(persist: &PersistHandle, message_uuid: uuid::Uuid) {
    if let Err(e) = persist.tell_sender_not_delivered(message_uuid).await {
        tracing::warn!(%message_uuid, error = %e, "telling the sender a message was given up on failed");
    }
}

fn log_redelivery_disabled(session_id: &str, message_uuid: uuid::Uuid) {
    tracing::debug!(
        session_id = %session_id,
        %message_uuid,
        "mailbox_redelivery disabled — dropping already-delivered reminder"
    );
}

fn log_redelivery_backoff_not_elapsed(
    session_id: &str,
    message_uuid: uuid::Uuid,
    delivery_attempts: i32,
) {
    tracing::debug!(
        session_id = %session_id,
        %message_uuid,
        delivery_attempts,
        "mailbox redelivery deferred — backoff window not elapsed"
    );
}

fn log_redelivery_bumped(
    session_id: &str,
    message_uuid: uuid::Uuid,
    delivery_attempts: i32,
    dead_lettered: bool,
) {
    if dead_lettered {
        log_redelivery_dead_lettered(session_id, message_uuid, delivery_attempts);
    } else {
        log_redelivery_succeeded(session_id, message_uuid, delivery_attempts);
    }
}

fn log_redelivery_dead_lettered(
    session_id: &str,
    message_uuid: uuid::Uuid,
    delivery_attempts: i32,
) {
    tracing::info!(
        session_id = %session_id,
        %message_uuid,
        delivery_attempts,
        "mailbox message dead-lettered — redelivery cap reached"
    );
}

fn log_redelivery_succeeded(session_id: &str, message_uuid: uuid::Uuid, delivery_attempts: i32) {
    tracing::debug!(
        session_id = %session_id,
        %message_uuid,
        delivery_attempts,
        "mailbox message redelivered"
    );
}

fn log_redelivery_not_eligible(session_id: &str, message_uuid: uuid::Uuid) {
    tracing::debug!(
        session_id = %session_id,
        %message_uuid,
        "mailbox row not redelivery-eligible — dropping stale reminder"
    );
}

fn log_redelivery_bump_failed(session_id: &str, message_uuid: uuid::Uuid, e: &PersistError) {
    tracing::warn!(
        session_id = %session_id,
        %message_uuid,
        error = %e,
        "mailbox redelivery bump failed — requeueing"
    );
}
