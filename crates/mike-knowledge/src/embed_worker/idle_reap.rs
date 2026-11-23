use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::store::persistence::{PersistError, PersistHandle};

pub(super) const IDLE_TURN_THRESHOLD: Duration = Duration::from_secs(30 * 60);
const REAP_BACKSTOP: Duration = Duration::from_secs(600);
const REAP_RETRY: Duration = Duration::from_secs(30);
const PAST_THE_EDGE: Duration = Duration::from_secs(1);

fn is_stale(earliest: DateTime<Utc>, threshold: Duration, now: DateTime<Utc>) -> bool {
    chrono::Duration::from_std(threshold).is_ok_and(|threshold| earliest < now - threshold)
}

fn wait_until_stale(
    earliest: Option<DateTime<Utc>>,
    threshold: Duration,
    now: DateTime<Utc>,
) -> Duration {
    let Some(earliest) = earliest else {
        return REAP_BACKSTOP;
    };
    let stale_at = chrono::Duration::from_std(threshold).map(|threshold| earliest + threshold);
    let until = stale_at.map_or(REAP_BACKSTOP, |stale_at| {
        (stale_at - now).to_std().unwrap_or(Duration::ZERO)
    });
    (until + PAST_THE_EDGE).min(REAP_BACKSTOP)
}

pub(super) async fn reap_when_due(persist: &PersistHandle) -> Duration {
    let Some(earliest) = read_earliest(persist).await else {
        return REAP_BACKSTOP;
    };
    let now = Utc::now();
    if !earliest.is_some_and(|at| is_stale(at, IDLE_TURN_THRESHOLD, now)) {
        return wait_until_stale(earliest, IDLE_TURN_THRESHOLD, now);
    }
    reap_idle_turns(persist).await;
    let Some(after) = read_earliest(persist).await else {
        return REAP_BACKSTOP;
    };
    wait_until_stale(after, IDLE_TURN_THRESHOLD, Utc::now()).max(REAP_RETRY)
}

async fn read_earliest(persist: &PersistHandle) -> Option<Option<DateTime<Utc>>> {
    match persist.earliest_open_turn_seen_at().await {
        Ok(earliest) => Some(earliest),
        Err(error) => {
            log_read_failed(&error);
            None
        }
    }
}

async fn reap_idle_turns(persist: &PersistHandle) {
    match persist
        .reap_idle_open_turns(IDLE_TURN_THRESHOLD, Utc::now())
        .await
    {
        Ok(reaped) if !reaped.is_empty() => {
            persist.note_turn_embed_owed();
            log_reaped(reaped.len());
        }
        Ok(_) => {}
        Err(e) => log_reap_failed(&e),
    }
}

fn log_read_failed(error: &PersistError) {
    tracing::warn!(%error, "embed_backfill: reading the oldest open turn failed; will look again later");
}

fn log_reaped(count: usize) {
    tracing::info!(
        turns = count,
        "embed_backfill: reaped {} idle open turn(s)",
        count
    );
}

fn log_reap_failed(e: &PersistError) {
    tracing::warn!(error = %e, "embed_backfill: idle-turn reap failed — will retry next interval");
}

#[cfg(test)]
#[path = "idle_reap_tests.rs"]
mod idle_reap_tests;
