use super::requeue_reminder;
use crate::reminders::inject::{InjectBookkeeping, inject_system_reminder};
use crate::state::AppState;
use crate::store::models::{ReminderSource, SystemReminder};
use crate::store::persistence::PersistError;
use crate::store::persistence::launch_repo::LaunchPersist as _;

pub(in crate::steerer::stage) async fn inject_or_requeue(
    state: &AppState,
    session_id: &str,
    reminder: SystemReminder,
    requeue_for_future_redelivery: bool,
    bk: &mut InjectBookkeeping<'_>,
    injected_list: &mut Vec<SystemReminder>,
) -> Option<&'static str> {
    if let Some(reason) = state
        .injection_gate
        .admit(session_id, &reminder)
        .suppression_reason()
    {
        log_gate_dropped(session_id, &reminder, reason);
        return Some(reason);
    }

    if inject_system_reminder(&reminder, bk) {
        state.injection_gate.record_delivery(session_id, &reminder);
        record_injected(
            state,
            session_id,
            requeue_for_future_redelivery,
            reminder,
            injected_list,
        )
        .await;
        return None;
    }

    log_inject_failed(session_id, &reminder);
    unclaim_mailbox_message_after_failed_inject(state, session_id, &reminder).await;
    requeue_reminder(state, session_id, reminder).await;
    None
}

async fn record_injected(
    state: &AppState,
    session_id: &str,
    requeue_for_future_redelivery: bool,
    reminder: SystemReminder,
    injected_list: &mut Vec<SystemReminder>,
) {
    if requeue_for_future_redelivery {
        state
            .store
            .write()
            .await
            .enqueue_reminder(session_id, reminder.clone());
    }
    injected_list.push(reminder);
}

fn log_gate_dropped(session_id: &str, reminder: &SystemReminder, reason: &str) {
    tracing::debug!(
        session_id = %session_id,
        reason,
        source = ?reminder.source,
        "injection gate dropped a queued reminder instead of redelivering it"
    );
}

fn log_inject_failed(session_id: &str, reminder: &SystemReminder) {
    tracing::warn!(
        session_id = %session_id,
        source = ?reminder.source,
        "steerer reminder inject failed — requeueing"
    );
}

async fn unclaim_mailbox_message_after_failed_inject(
    state: &AppState,
    session_id: &str,
    reminder: &SystemReminder,
) {
    let ReminderSource::SessionMailbox { message_uuid, .. } = &reminder.source else {
        return;
    };
    let Some(persist) = &state.persist else {
        return;
    };
    if let Err(e) = persist.launch().unclaim_message(*message_uuid).await {
        log_mailbox_unclaim_failed(session_id, *message_uuid, &e);
    }
}

fn log_mailbox_unclaim_failed(session_id: &str, message_uuid: uuid::Uuid, e: &PersistError) {
    tracing::warn!(
        session_id = %session_id,
        %message_uuid,
        error = %e,
        "mailbox unclaim after failed inject failed"
    );
}
