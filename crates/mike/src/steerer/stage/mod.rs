pub(crate) mod client_served;
mod judge;
mod recap;
mod reminder_dispatch;
mod unqueued_mail;

#[cfg(test)]
mod drain_tests;

use crate::compress::manifest::{AppliedStage, ManifestBuilder, UnchangedStage};
use crate::config::AppState;
use crate::extract::request::TurnOrigin;
use crate::reminders::inject::InjectBookkeeping;
use crate::store::models::{ReminderSource, SystemReminder};
use reminder_dispatch::{
    MailboxClaim, SteererGate, claim_mailbox_reminder, inject_or_requeue, is_advisory,
    request_is_mid_turn, steerer_reminder_gate,
};

pub(crate) async fn drain(
    state: &AppState,
    session_id: &str,
    turn_origin: &TurnOrigin,
    human_prompt: &str,
    bk: &mut InjectBookkeeping<'_>,
    manifest: &mut ManifestBuilder,
) -> Vec<SystemReminder> {
    if !matches!(turn_origin, TurnOrigin::Primary | TurnOrigin::PrimaryFirst) {
        return Vec::new();
    }

    let mut injected_list: Vec<SystemReminder> = Vec::new();

    if let Some(reminder) = run_same_request_judge(state, session_id, human_prompt, bk).await {
        injected_list.push(reminder);
    }

    if recap::is_away_recap_call(bk.post_filter_body.as_ref()) {
        return injected_list;
    }

    let mailbox_served = client_served::mailbox_served(session_id);
    unqueued_mail::queue_mail_from_the_record(state, session_id, mailbox_served).await;
    let pending = claim_pending(state, session_id, mailbox_served).await;
    let pending_count = pending.len();
    let mut gate_suppressed: Vec<&'static str> = Vec::new();
    let max_mailbox_deliveries =
        usize::try_from(state.config().mailbox_max_deliveries_per_turn).unwrap_or(usize::MAX);
    let skip_count = mailbox_skip_count(&pending, max_mailbox_deliveries);
    let mut mailbox_seen = 0usize;
    let mid_turn =
        pending.iter().any(is_advisory) && request_is_mid_turn(bk.post_filter_body.as_ref());

    injected_list.reserve(pending.len());
    for reminder in pending {
        let (reminder, requeue_for_future_redelivery) = match claim_mailbox_reminder(
            state,
            session_id,
            reminder,
            &mut mailbox_seen,
            skip_count,
        )
        .await
        {
            MailboxClaim::NotMailbox(reminder) => (reminder, false),
            MailboxClaim::Proceed(reminder, requeue) => (reminder, requeue),
            MailboxClaim::Skip => continue,
        };

        match steerer_reminder_gate(state, session_id, &reminder, mid_turn).await {
            SteererGate::Stale => continue,
            SteererGate::PushInFlight | SteererGate::WaitForTurnEnd => {
                state
                    .store
                    .write()
                    .await
                    .requeue_reminder(session_id, reminder);
                continue;
            }
            SteererGate::Deliver => {}
        }

        if let Some(reason) = inject_or_requeue(
            state,
            session_id,
            reminder,
            requeue_for_future_redelivery,
            bk,
            &mut injected_list,
        )
        .await
        {
            gate_suppressed.push(reason);
        }
    }

    record_drain_outcome(
        manifest,
        bk.post_filter_body.as_ref(),
        DrainTally {
            injected: injected_list.len(),
            pending: pending_count,
            gate_suppressed: &gate_suppressed,
            mailbox_served,
        },
    );

    injected_list
}

async fn claim_pending(
    state: &AppState,
    session_id: &str,
    mailbox_served: bool,
) -> Vec<SystemReminder> {
    let mut store = state.store.write().await;
    let claimed = store.claim_reminders(session_id);
    if !mailbox_served {
        return claimed;
    }
    let (rest, held) = client_served::split_mailbox(claimed);
    for reminder in held.into_iter().rev() {
        store.requeue_reminder(session_id, reminder);
    }
    rest
}

struct DrainTally<'a> {
    injected: usize,
    pending: usize,
    gate_suppressed: &'a [&'static str],
    mailbox_served: bool,
}

fn record_drain_outcome(manifest: &mut ManifestBuilder, new_bytes: &[u8], tally: DrainTally<'_>) {
    let resolved_parameters = serde_json::json!({
        "reminder_count": tally.injected,
        "pending_count": tally.pending,
        "gate_suppressed": tally.gate_suppressed,
        "mailbox_served_by_client": tally.mailbox_served,
    });
    if tally.injected == 0 {
        manifest.record_unchanged_with(UnchangedStage {
            stage: "steerer",
            policy_id: "steerer",
            reason_code: nothing_injected_reason(tally.pending, tally.gate_suppressed),
            resolved_parameters,
            targets: Vec::new(),
            detail: None,
        });
        return;
    }
    manifest.record_applied(AppliedStage {
        stage: "steerer",
        policy_id: "steerer",
        policy_version: 1,
        resolved_parameters,
        new_bytes,
        targets: Vec::new(),
        reason_code: "steerer_reminder_drained",
        detail: Some(format!(
            "injected {} reminder(s) from the steerer/mailbox queue",
            tally.injected
        )),
    });
}

fn nothing_injected_reason(pending: usize, gate_suppressed: &[&'static str]) -> &'static str {
    if !gate_suppressed.is_empty() {
        "steerer_gate_suppressed"
    } else if pending == 0 {
        "steerer_queue_empty"
    } else {
        "steerer_reminders_deferred"
    }
}

async fn run_same_request_judge(
    state: &AppState,
    session_id: &str,
    human_prompt: &str,
    bk: &mut InjectBookkeeping<'_>,
) -> Option<SystemReminder> {
    let reminder = judge::judge_turn_close_before_drain(state, session_id, human_prompt).await?;
    if crate::reminders::inject::inject_system_reminder(&reminder, bk) {
        return Some(reminder);
    }
    tracing::warn!(
        session_id = %session_id,
        source = ?reminder.source,
        "same-request steerer fire failed to inject — enqueued for the next drain"
    );
    state
        .store
        .write()
        .await
        .enqueue_reminder(session_id, reminder);
    None
}

fn mailbox_skip_count(pending: &[SystemReminder], max_per_turn: usize) -> usize {
    let mailbox_total = pending
        .iter()
        .filter(|r| matches!(r.source, ReminderSource::SessionMailbox { .. }))
        .count();
    mailbox_total.saturating_sub(max_per_turn)
}
