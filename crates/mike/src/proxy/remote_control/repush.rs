use std::collections::HashSet;
use std::time::Duration;

use tokio::time::{MissedTickBehavior, interval};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::config::AppState;
use crate::store::models::SystemReminder;

use super::inject::{PushDeps, push_source, try_push_reminder};

const SWEEP_INTERVAL: Duration = Duration::from_secs(10);

pub(crate) fn spawn_repush_sweep(state: &AppState) {
    if state.persist().is_none() {
        log_not_spawning_no_persistence();
        return;
    }
    let deps = state.push_deps();
    let shutdown = state.shutdown();
    state.tracker.spawn(run(deps, shutdown));
    log_spawned();
}

fn log_not_spawning_no_persistence() {
    tracing::debug!("remote control repush sweep: no persistence — not spawning");
}

fn log_spawned() {
    tracing::info!(
        interval_secs = SWEEP_INTERVAL.as_secs(),
        "remote control repush sweep: spawned"
    );
}

pub(super) fn spawn_repush_for_stream(state: &AppState, cse_id: String) {
    let deps = state.push_deps();
    state.tracker.spawn(async move {
        repush_pending(&deps, Some(&cse_id)).await;
    });
}

async fn run(deps: PushDeps, shutdown: CancellationToken) {
    let mut ticker = interval(SWEEP_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            () = shutdown.cancelled() => return,
            _ = ticker.tick() => {
                repush_pending(&deps, None).await;
            }
        }
    }
}

pub(crate) async fn repush_pending(deps: &PushDeps, cse_id: Option<&str>) -> usize {
    let candidates = wakeable_sessions(deps).await;
    if candidates.is_empty() {
        return 0;
    }
    let mut pushed = 0;
    for recipient in recipients_with_stream(deps, &candidates, cse_id).await {
        pushed += repush_session(deps, recipient).await;
    }
    pushed
}

fn is_wakeable(deps: &PushDeps, session_id: &str, reminder: &SystemReminder) -> bool {
    push_source(&reminder.source).is_some() && deps.gate.admit(session_id, reminder).may_wake()
}

async fn wakeable_sessions(deps: &PushDeps) -> Vec<Uuid> {
    let store = deps.store.read().await;
    store
        .sessions_with_pending_reminders()
        .into_iter()
        .filter(|session_id| {
            store
                .peek_reminders(session_id)
                .iter()
                .any(|reminder| is_wakeable(deps, session_id, reminder))
        })
        .filter_map(|session_id| session_id.parse().ok())
        .collect()
}

async fn recipients_with_stream(
    deps: &PushDeps,
    candidates: &[Uuid],
    cse_id: Option<&str>,
) -> Vec<Uuid> {
    let Some(handle) = deps.persist.as_ref() else {
        return Vec::new();
    };
    let channels = match handle.fetch_channel_snapshots(candidates).await {
        Ok(channels) => channels,
        Err(error) => {
            log_channel_lookup_failed(&error);
            return Vec::new();
        }
    };
    let recipients: HashSet<Uuid> = channels
        .into_iter()
        .filter(|channel| cse_id.is_none_or(|wanted| wanted == channel.cse_id))
        .filter(|channel| {
            deps.remote_control_streams
                .sender(&channel.cse_id)
                .is_some()
        })
        .filter_map(|channel| channel.local_session_uuid)
        .collect();
    recipients.into_iter().collect()
}

fn log_channel_lookup_failed(error: &dyn std::fmt::Display) {
    tracing::warn!(%error, "remote control repush: channel lookup failed");
}

async fn repush_session(deps: &PushDeps, recipient: Uuid) -> usize {
    let session_id = recipient.to_string();
    let taken = deps
        .store
        .write()
        .await
        .take_reminders_where(&session_id, |reminder| {
            is_wakeable(deps, &session_id, reminder)
        });
    let (pushed, failed) = push_in_order(deps, recipient, taken).await;
    requeue_front(deps, &session_id, failed).await;
    log_pushed(recipient, pushed);
    pushed
}

async fn push_in_order(
    deps: &PushDeps,
    recipient: Uuid,
    reminders: Vec<SystemReminder>,
) -> (usize, Vec<SystemReminder>) {
    let mut failed = Vec::new();
    let mut pushed = 0;
    for reminder in reminders {
        if try_push_reminder(deps, recipient, &reminder).await {
            pushed += 1;
        } else {
            failed.push(reminder);
        }
    }
    (pushed, failed)
}

async fn requeue_front(deps: &PushDeps, session_id: &str, failed: Vec<SystemReminder>) {
    if failed.is_empty() {
        return;
    }
    let mut store = deps.store.write().await;
    for reminder in failed.into_iter().rev() {
        store.requeue_reminder(session_id, reminder);
    }
}

fn log_pushed(recipient: Uuid, pushed: usize) {
    if pushed > 0 {
        tracing::info!(
            %recipient,
            pushed,
            "remote control: pushed queued reminders once the session's stream was available"
        );
    }
}
