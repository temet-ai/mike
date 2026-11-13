use super::*;
use arc_swap::ArcSwapOption;
use std::sync::Arc;
use std::time::Duration;

pub const REFRESH_INTERVAL: Duration = Duration::from_secs(60);
pub const REFRESH_READ_DEADLINE: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, PartialEq)]
pub struct Stored<T> {
    pub value: T,
    pub refreshed_at: chrono::DateTime<chrono::Utc>,
}

impl<T> Stored<T> {
    pub fn age_secs(&self, now: chrono::DateTime<chrono::Utc>) -> i64 {
        (now - self.refreshed_at).num_seconds().max(0)
    }
}

pub fn renew<T: Clone>(
    previous: Option<&Stored<T>>,
    fresh: Option<T>,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<Stored<T>> {
    match fresh {
        Some(value) => Some(Stored {
            value,
            refreshed_at: now,
        }),
        None => previous.cloned(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurnCounts {
    pub closed_unembedded: Option<i64>,
    pub open_unembedded: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StoredSections {
    pub knowledge: Option<Stored<serde_json::Value>>,
    pub turns: Option<Stored<TurnCounts>>,
    pub capture_activity: Option<Stored<Vec<gap::SessionActivity>>>,
}

#[derive(Default)]
pub struct Readings {
    pub knowledge: Option<serde_json::Value>,
    pub turns: Option<TurnCounts>,
    pub capture_activity: Option<Vec<gap::SessionActivity>>,
}

impl StoredSections {
    pub fn renewed(
        &self,
        readings: Readings,
        now: chrono::DateTime<chrono::Utc>,
    ) -> StoredSections {
        StoredSections {
            knowledge: renew(self.knowledge.as_ref(), readings.knowledge, now),
            turns: renew(self.turns.as_ref(), readings.turns, now),
            capture_activity: renew(
                self.capture_activity.as_ref(),
                readings.capture_activity,
                now,
            ),
        }
    }
}

static STORED: ArcSwapOption<StoredSections> = ArcSwapOption::const_empty();

pub fn current() -> Arc<StoredSections> {
    STORED.load_full().unwrap_or_default()
}

pub fn publish(sections: StoredSections) {
    STORED.store(Some(Arc::new(sections)));
}

pub async fn refresh_on_ticks<S, F, Fut>(
    shutdown: tokio_util::sync::CancellationToken,
    mut ticks: S,
    mut refresh: F,
) where
    S: tokio_stream::Stream + Unpin,
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    use tokio_stream::StreamExt;
    loop {
        tokio::select! {
            biased;
            () = shutdown.cancelled() => break,
            tick = ticks.next() => {
                if tick.is_none() {
                    break;
                }
                refresh().await;
            }
        }
    }
}

fn refresh_ticks(interval: Duration) -> tokio_stream::wrappers::IntervalStream {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    tokio_stream::wrappers::IntervalStream::new(ticker)
}

fn read_failed<T>(error: &impl std::fmt::Display, what: &'static str) -> Option<T> {
    tracing::warn!(%error, what, "health refresher: read failed");
    None
}

fn read_timed_out<T>(what: &'static str) -> Option<T> {
    tracing::warn!(what, "health refresher: read timed out");
    None
}

async fn read_bounded<T>(
    work: impl std::future::Future<Output = Result<T, impl std::fmt::Display>>,
    deadline: impl std::future::Future<Output = ()>,
    what: &'static str,
) -> Option<T> {
    match probe::bounded(work, deadline).await {
        probe::Bounded::Done(Ok(value)) => Some(value),
        probe::Bounded::Done(Err(error)) => read_failed(&error, what),
        probe::Bounded::TimedOut => read_timed_out(what),
    }
}

fn read_deadline() -> tokio::time::Sleep {
    tokio::time::sleep(REFRESH_READ_DEADLINE)
}

async fn read_turns(persist: &crate::store::persistence::PersistHandle) -> Option<TurnCounts> {
    let closed = read_bounded(
        persist.count_pending_embedding_turns(),
        read_deadline(),
        "closed turns",
    )
    .await;
    let open = read_bounded(
        persist.count_open_unembedded_turns(),
        read_deadline(),
        "open turns",
    )
    .await;
    if closed.is_none() && open.is_none() {
        return None;
    }
    Some(TurnCounts {
        closed_unembedded: closed,
        open_unembedded: open,
    })
}

async fn read_knowledge(state: &AppState) -> Option<serde_json::Value> {
    if !state.embedding_service().is_available() {
        return None;
    }
    match probe::within(
        crate::api::project_knowledge::embedding_capability(state),
        REFRESH_READ_DEADLINE,
    )
    .await
    {
        probe::Bounded::Done(row) => Some(serde_json::json!(row)),
        probe::Bounded::TimedOut => {
            tracing::warn!("health refresher: knowledge coverage timed out");
            None
        }
    }
}

pub async fn read_all(state: &AppState) -> Readings {
    let Some(persist) = state.persist() else {
        return Readings::default();
    };
    Readings {
        knowledge: read_knowledge(state).await,
        turns: read_turns(persist).await,
        capture_activity: gap::read_activity(persist, chrono::Utc::now()).await,
    }
}

pub async fn note_capture_outage(state: &AppState) {
    let (reachable, capture) = tokio::join!(
        probe::probe_persistence(state.persist(), probe::CAPABILITY_PROBE_DEADLINE),
        probe::probe_latest_capture(state.persist(), probe::CAPABILITY_PROBE_DEADLINE),
    );
    if reachable == probe::ProbeState::NotConfigured {
        return;
    }
    let persistence = persistence_health(state, reachable, capture);
    let failure = if !persistence.reachable {
        Some("the database does not answer")
    } else if persistence.capture_stalled {
        Some("the database answers but captures are not being written")
    } else {
        None
    };
    capture_outage::note_reading(failure);
}

pub async fn refresh_once(state: &AppState) {
    note_capture_outage(state).await;
    let readings = read_all(state).await;
    let next = current().renewed(readings, chrono::Utc::now());
    publish(next);
}

pub fn spawn_health_refresher(state: &AppState) {
    if state.persist().is_none() {
        return;
    }
    let owned = state.clone();
    let shutdown = state.shutdown();
    state.tracker.spawn(async move {
        refresh_on_ticks(shutdown, refresh_ticks(REFRESH_INTERVAL), || {
            refresh_once(&owned)
        })
        .await;
    });
}

#[cfg(test)]
mod tests;
