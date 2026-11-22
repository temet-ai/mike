use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::time::Duration;

use tokio::time::{MissedTickBehavior, interval};

use crate::base_state::BaseState;
use crate::change_feed::{ChangeFeed, Generation};
use crate::store::models::proxy_event::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch::gone_closure::GoneSweep;

const REAP_INTERVAL: Duration = Duration::from_secs(300);

/// How long gone-closure goes without looking when nothing says a launch can
/// close: a registration released on another machine is caught within this.
const GONE_BACKSTOP: Duration = Duration::from_secs(600);
const GONE_WATCHED: [RowChangedTable; 1] = [RowChangedTable::LaunchLaunches];

/// Decides whether a reaper tick sweeps for gone launches. A launch closes when
/// its registration is released, or when time passes with its session quiet; the
/// sweep reports the next such time, so between them it waits for a launch
/// change, a release in this process, or the backstop.
#[derive(Debug, Default)]
pub struct GoneGate {
    since: Generation,
    next_due: Option<chrono::DateTime<chrono::Utc>>,
    last: Option<std::time::Instant>,
}

impl GoneGate {
    pub fn should_sweep(
        &self,
        persist: &PersistHandle,
        feed: &ChangeFeed,
        now: chrono::DateTime<chrono::Utc>,
    ) -> bool {
        let released = persist.take_registration_released();
        let backstop = self.last.is_none_or(|last| last.elapsed() >= GONE_BACKSTOP);
        let due = self.next_due.is_some_and(|due| due <= now);
        released
            || backstop
            || due
            || !feed.is_fed()
            || feed.changed_since(&GONE_WATCHED, self.since)
    }

    fn swept(&mut self, since: Generation, next_due: Option<chrono::DateTime<chrono::Utc>>) {
        self.since = since;
        self.next_due = next_due;
        self.last = Some(std::time::Instant::now());
    }
}

pub async fn close_gone_launches(persist: &PersistHandle) -> Option<GoneSweep> {
    let quiet_since = chrono::Utc::now() - crate::workspace_reaper::LAPSED_LEASE_GRACE;
    match persist.launch().close_gone_launches(quiet_since).await {
        Ok(sweep) => {
            if sweep.closed > 0 {
                log_closed(sweep.closed);
            }
            Some(sweep)
        }
        Err(error) => {
            log_close_failed(&error);
            None
        }
    }
}

async fn sweep_if_due(persist: &PersistHandle, feed: &ChangeFeed, gate: &mut GoneGate) {
    if !gate.should_sweep(persist, feed, chrono::Utc::now()) {
        return;
    }
    let since = feed.generation();
    if let Some(sweep) = close_gone_launches(persist).await {
        gate.swept(since, sweep.next_due);
    }
}

fn log_closed(closed: u64) {
    tracing::info!(
        closed,
        "mailbox_reaper: closed launches whose session is gone"
    );
}

fn log_close_failed(error: &crate::store::persistence::PersistError) {
    tracing::warn!(%error, "mailbox_reaper: could not close launches whose session is gone");
}

pub fn spawn_mailbox_reaper(state: &BaseState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::debug!("mailbox_reaper: no persistence — not spawning");
        return;
    };
    let state = state.clone();
    let shutdown = state.shutdown();
    let tracker = state.tracker.clone();

    tracker.spawn(async move {
        let mut gone = GoneGate::default();
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
                    sweep_if_due(&persist, &state.change_feed, &mut gone).await;
                    let notice_cutoff = chrono::Utc::now() - chrono::Duration::minutes(5);
                    if let Err(err) = persist.launch().tell_inactive_senders(notice_cutoff).await {
                        tracing::warn!(error = %err, "mailbox_reaper: could not notify waiting senders");
                    }
                    let ttl_minutes = state.config().mailbox_delivery_ttl_minutes;
                    let cutoff = chrono::Utc::now() - chrono::Duration::minutes(ttl_minutes);
                    let activity_cutoff = cutoff;
                    match persist
                        .launch().dead_letter_expired_undelivered(cutoff, activity_cutoff)
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
