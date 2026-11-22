use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::time::Duration;

use tokio::time::{MissedTickBehavior, interval};

use crate::base_state::BaseState;
use crate::change_feed::{ChangeFeed, Generation};
use crate::store::models::proxy_event::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch::gone_closure::GoneSweep;

const REAP_INTERVAL: Duration = Duration::from_secs(300);
const CHANGE_WAIT: Duration = Duration::from_secs(3600);
const NOTICE_DELAY: chrono::Duration = chrono::Duration::minutes(5);
const MAIL: [RowChangedTable; 1] = [RowChangedTable::MailboxMessages];

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
    let feed = state.change_feed.clone();

    tracker.spawn(async move {
        let mut gone = GoneGate::default();
        let mut ticker = interval(REAP_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut due = MailDue::default();
        loop {
            tokio::select! {
                biased;

                _ = shutdown.cancelled() => {
                    tracing::debug!("mailbox_reaper: shutdown signalled, exiting");
                    break;
                }
                () = due.next_change(&feed) => {}
                _ = ticker.tick() => {
                    sweep_if_due(&persist, &feed, &mut gone).await;
                    if due.is_due(&feed, chrono::Utc::now()) {
                        due.checked(&feed);
                        sweep_mail(&state, &persist, &mut due).await;
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

async fn sweep_mail(state: &BaseState, persist: &PersistHandle, due: &mut MailDue) {
    let earliest = match persist.launch().earliest_undelivered().await {
        Ok(earliest) => earliest,
        Err(error) => {
            log_unread(&error);
            due.unknown();
            return;
        }
    };
    due.found(earliest);
    let now = chrono::Utc::now();
    let notice_cutoff = now - NOTICE_DELAY;
    if !older_than(earliest, notice_cutoff) {
        return;
    }
    if let Err(error) = persist.launch().tell_inactive_senders(notice_cutoff).await {
        log_notice_failed(&error);
    }
    let ttl_minutes = state.config().mailbox_delivery_ttl_minutes;
    let cutoff = now - chrono::Duration::minutes(ttl_minutes);
    if older_than(earliest, cutoff) {
        let reaped = persist
            .launch()
            .dead_letter_expired_undelivered(cutoff, cutoff)
            .await;
        log_reaped(reaped.map(|outcome| outcome.dead_lettered), ttl_minutes);
    }
}

fn older_than(
    earliest: Option<chrono::DateTime<chrono::Utc>>,
    cutoff: chrono::DateTime<chrono::Utc>,
) -> bool {
    earliest.is_some_and(|at| at < cutoff)
}

fn log_unread(error: &crate::store::persistence::PersistError) {
    tracing::warn!(%error, "mailbox_reaper: could not read undelivered mail");
}

fn log_notice_failed(error: &crate::store::persistence::PersistError) {
    tracing::warn!(%error, "mailbox_reaper: could not notify waiting senders");
}

fn log_reaped(reaped: Result<i64, crate::store::persistence::PersistError>, ttl_minutes: i64) {
    match reaped {
        Ok(0) => {}
        Ok(dead_lettered) => log_dead_lettered(dead_lettered, ttl_minutes),
        Err(e) => tracing::warn!(error = %e, "mailbox_reaper: sweep failed"),
    }
}

fn log_dead_lettered(dead_lettered: i64, ttl_minutes: i64) {
    tracing::info!(
        dead_lettered,
        ttl_minutes,
        "mailbox_reaper: dead-lettered stranded messages"
    );
}

#[derive(Debug, Default)]
struct MailDue {
    known: bool,
    earliest: Option<chrono::DateTime<chrono::Utc>>,
    first_change: Option<chrono::DateTime<chrono::Utc>>,
    since: Generation,
}

impl MailDue {
    async fn next_change(&mut self, feed: &ChangeFeed) {
        if self.first_change.is_some() || !feed.is_fed() {
            return std::future::pending().await;
        }
        feed.wait_for_change(&MAIL, self.since, CHANGE_WAIT).await;
        if feed.changed_since(&MAIL, self.since) {
            self.first_change = Some(chrono::Utc::now());
        }
    }

    fn is_due(&self, feed: &ChangeFeed, now: chrono::DateTime<chrono::Utc>) -> bool {
        let ripe = |at: Option<chrono::DateTime<chrono::Utc>>| {
            at.is_some_and(|at| at + NOTICE_DELAY <= now)
        };
        !self.known || !feed.is_fed() || ripe(self.earliest) || ripe(self.first_change)
    }

    fn checked(&mut self, feed: &ChangeFeed) {
        self.since = feed.generation();
        self.first_change = None;
    }

    fn found(&mut self, earliest: Option<chrono::DateTime<chrono::Utc>>) {
        self.known = true;
        self.earliest = earliest;
    }

    fn unknown(&mut self) {
        self.known = false;
    }
}

#[cfg(test)]
#[path = "mailbox_reaper_tests.rs"]
mod tests;
