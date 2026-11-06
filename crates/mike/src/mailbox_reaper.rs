use std::time::Duration;

use tokio::time::{MissedTickBehavior, interval};

use crate::config::AppState;

const REAP_INTERVAL: Duration = Duration::from_secs(300);

pub fn spawn_mailbox_reaper(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::debug!("mailbox_reaper: no persistence — not spawning");
        return;
    };
    let state = state.clone();
    let shutdown = state.shutdown();
    let tracker = state.tracker.clone();

    tracker.spawn(async move {
        let mut ticker = interval(REAP_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;

                _ = shutdown.cancelled() => {
                    tracing::debug!("mailbox_reaper: shutdown signalled, exiting");
                    break;
                }
                _ = ticker.tick() => {
                    let notice_cutoff = chrono::Utc::now() - chrono::Duration::minutes(5);
                    if let Err(err) = persist.tell_inactive_senders(notice_cutoff).await {
                        tracing::warn!(%err, "mailbox_reaper: could not notify waiting senders");
                    }
                    let ttl_minutes = state.config().mailbox_delivery_ttl_minutes;
                    let cutoff = chrono::Utc::now() - chrono::Duration::minutes(ttl_minutes);
                    let activity_cutoff = cutoff;
                    match persist
                        .dead_letter_expired_undelivered(cutoff, activity_cutoff)
                        .await
                    {
                        Ok(outcome) if outcome.dead_lettered > 0 => {
                            tracing::info!(
                                dead_lettered = outcome.dead_lettered,
                                ttl_minutes,
                                "mailbox_reaper: dead-lettered stranded messages"
                            );
                        }
                        Ok(_) => {}
                        Err(e) => {
                            tracing::warn!(error = %e, "mailbox_reaper: sweep failed");
                        }
                    }
                }
            }
        }
    });

    tracing::info!(
        interval_secs = REAP_INTERVAL.as_secs(),
        "mailbox_reaper: spawned"
    );
}
