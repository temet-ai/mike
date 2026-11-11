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
use crate::wright::Cadence;

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

pub(crate) use catalogue::{CHECKS, WatchContext, run_check};

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

pub fn spawn_wright_worker(state: &AppState) {
    if !state.feature_toggle_overlay.load().wright {
        tracing::debug!(
            reason = "disabled by feature toggle",
            "wright: not spawning"
        );
        return;
    }
    let Some(persist) = state.persist().cloned() else {
        tracing::debug!(reason = "no persistence", "wright: not spawning");
        return;
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
    tracing::info!(tick_secs = TICK.as_secs(), "wright: watch worker spawned");
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
    let mut events = Some(events);
    let mut next_regular = Instant::now();
    let mut event_due: Option<Instant> = None;
    while toggles.load().wright {
        let wake = event_due.map_or(next_regular, |due| due.min(next_regular));
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep_until(wake) => {}
            event = next_event(&mut events) => {
                if matches!(event, Err(RecvError::Closed)) {
                    events = None;
                } else if wants(&event) {
                    event_due.get_or_insert(Instant::now() + EVENT_DEBOUNCE);
                }
                continue;
            }
        }
        let now = Instant::now();
        if now >= next_regular {
            pass(&ctx, Pass::Regular).await;
            next_regular = now + TICK;
            event_due = None;
        } else if event_due.is_some_and(|due| now >= due) {
            pass(&ctx, Pass::Event).await;
            event_due = None;
        }
    }
    tracing::info!(
        "wright: disabled via feature toggle — exiting (a UI flip back on needs a restart)"
    );
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

pub(crate) async fn pass(ctx: &WatchContext, kind: Pass) {
    if !ctx.persist.holds_worker_role(LEASE_ROLE, TICK).await {
        return;
    }
    let cadences = cadences_for(ctx, kind).await;
    for spec in catalogue::due(&cadences) {
        match run_check(spec, ctx).await {
            Ok((outcome, recorded)) => tracing::info!(
                check = spec.name,
                outcome = outcome.code(),
                opened = recorded.opened,
                recurred = recorded.recurred,
                healthy = recorded.healthy,
                "wright: check ran"
            ),
            Err(e) => {
                tracing::warn!(error = %e, check = spec.name, "wright: recording a check run failed")
            }
        }
    }
}
