use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::config::AppState;
use crate::feature_toggles::FeatureToggles;
use crate::store::models::ProxyEvent;
use crate::store::persistence::PersistError;
use crate::store::persistence::wright::RecordedRun;
use crate::wright::{Cadence, CheckOutcome};

pub(crate) mod bounded;
pub(crate) mod catalogue;
pub(crate) mod forks;
pub(crate) mod logs;

#[cfg(test)]
mod catalogue_tests;
#[cfg(test)]
mod checks_tests;
#[cfg(test)]
mod fixtures_tests;
#[cfg(test)]
mod forks_tests;
#[cfg(test)]
mod logs_tests;
#[cfg(test)]
mod problems_tests;

pub(crate) use catalogue::{WatchContext, run_check};

const LEASE_ROLE: &str = "wright_watch";
const TICK: Duration = Duration::from_secs(5 * 60);
const EVENT_DEBOUNCE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pass {
    Event,
    Regular,
}

pub(crate) fn daily_pass_due(last: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    last.is_none_or(|last| last.date_naive() < now.date_naive())
}

fn log_not_spawned(reason: &str) {
    tracing::debug!(reason, "wright: not spawning");
}

fn log_spawned() {
    tracing::info!(tick_secs = TICK.as_secs(), "wright: watch worker spawned");
}

fn log_disabled() {
    tracing::info!(
        "wright: disabled via feature toggle — exiting (a UI flip back on needs a restart)"
    );
}

pub fn spawn_wright_worker(state: &AppState) {
    if !state.feature_toggle_overlay.load().wright {
        return log_not_spawned("disabled by feature toggle");
    }
    let Some(persist) = state.persist().cloned() else {
        return log_not_spawned("no persistence");
    };
    let ctx = WatchContext {
        persist,
        log_dir: state.config().log_dir.clone(),
        machine: None,
    };
    let toggles = state.feature_toggle_overlay.clone();
    let events = state.event_tx.subscribe();
    state
        .tracker
        .spawn(run(ctx, toggles, events, state.shutdown()));
    log_spawned();
}

fn wants(event: &Result<ProxyEvent, RecvError>) -> bool {
    matches!(
        event,
        Ok(ProxyEvent::ResponseError { .. }) | Err(RecvError::Lagged(_))
    )
}

async fn next_event(events: &mut Option<Receiver<ProxyEvent>>) -> Result<ProxyEvent, RecvError> {
    match events {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

async fn run(
    mut ctx: WatchContext,
    toggles: Arc<ArcSwap<FeatureToggles>>,
    events: Receiver<ProxyEvent>,
    shutdown: CancellationToken,
) {
    ctx.machine = crate::machine_identity::load_default()
        .await
        .ok()
        .map(|m| m.machine_uuid);
    let mut schedule = Schedule {
        events: Some(events),
        next_regular: Instant::now(),
        event_due: None,
    };
    while toggles.load().wright {
        let wake = schedule.wake();
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep_until(wake) => {}
            event = next_event(&mut schedule.events) => {
                schedule.note(&event);
                continue;
            }
        }
        if let Some(kind) = schedule.due(Instant::now()) {
            pass(&ctx, kind).await;
        }
    }
    log_disabled();
}

struct Schedule {
    events: Option<Receiver<ProxyEvent>>,
    next_regular: Instant,
    event_due: Option<Instant>,
}

impl Schedule {
    fn wake(&self) -> Instant {
        self.event_due
            .map_or(self.next_regular, |due| due.min(self.next_regular))
    }

    fn note(&mut self, event: &Result<ProxyEvent, RecvError>) {
        if matches!(event, Err(RecvError::Closed)) {
            self.events = None;
        } else if wants(event) {
            self.event_due
                .get_or_insert(Instant::now() + EVENT_DEBOUNCE);
        }
    }

    fn due(&mut self, now: Instant) -> Option<Pass> {
        if now >= self.next_regular {
            self.next_regular = now + TICK;
            self.event_due = None;
            return Some(Pass::Regular);
        }
        if self.event_due.is_some_and(|due| now >= due) {
            self.event_due = None;
            return Some(Pass::Event);
        }
        None
    }
}

async fn cadences_for(ctx: &WatchContext, kind: Pass) -> Vec<Cadence> {
    match kind {
        Pass::Event => vec![Cadence::Event],
        Pass::Regular => {
            let mut due = vec![Cadence::Event, Cadence::Regular];
            match ctx.persist.last_wright_daily_pass().await {
                Ok(last) if daily_pass_due(last, Utc::now()) => due.push(Cadence::Daily),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "wright: cannot read the last daily pass"),
            }
            due
        }
    }
}

fn log_ran(check: &str, outcome: &CheckOutcome, recorded: &RecordedRun) {
    tracing::info!(
        check,
        outcome = outcome.code(),
        opened = recorded.opened,
        recurred = recorded.recurred,
        healthy = recorded.healthy,
        "wright: check ran"
    );
}

fn log_record_failed(check: &str, e: &PersistError) {
    tracing::warn!(error = %e, check, "wright: recording a check run failed");
}

pub(crate) async fn pass(ctx: &WatchContext, kind: Pass) {
    if !ctx.persist.holds_worker_role(LEASE_ROLE, TICK).await {
        return;
    }
    let cadences: Vec<String> = cadences_for(ctx, kind)
        .await
        .into_iter()
        .map(|cadence| cadence.code().to_owned())
        .collect();
    let checks = match ctx.persist.enabled_wright_checks(&cadences).await {
        Ok(checks) => checks,
        Err(e) => return tracing::warn!(error = %e, "wright: cannot load the enabled checks"),
    };
    for check in &checks {
        match run_check(check, ctx).await {
            Ok((outcome, recorded)) => log_ran(&check.name, &outcome, &recorded),
            Err(e) => log_record_failed(&check.name, &e),
        }
    }
}
