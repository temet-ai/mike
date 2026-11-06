use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use dashmap::DashMap;

use super::managed_command::Context;
use super::owned_execution::Owner;
use crate::store::persistence::PersistHandle;

const FALLBACK_PATIENCE: Duration = Duration::from_secs(1800);
const PATIENCE_FLOOR: Duration = Duration::from_secs(600);
const PATIENCE_CEILING: Duration = Duration::from_secs(7200);
const PATIENCE_PER_RUN: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Lane {
    Gate,
    Check,
}

impl Lane {
    fn slots(self) -> usize {
        match self {
            Self::Gate => 1,
            Self::Check => super::gate_slot::CHECK_SLOT_NAMES.len(),
        }
    }
}

pub(super) struct Enlisted<'a> {
    pub repo: &'a str,
    pub lane: Lane,
    pub ledger: &'a str,
    pub seat: &'a str,
    pub state: &'a str,
}

struct Entry {
    lane: (String, Lane),
    obligation: String,
    seat: String,
    state: String,
    executing: bool,
    owner: Owner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Depth {
    pub waiting: usize,
    pub executing: usize,
    pub slots: usize,
}

fn entries() -> &'static DashMap<i64, Entry> {
    static ENTRIES: OnceLock<DashMap<i64, Entry>> = OnceLock::new();
    ENTRIES.get_or_init(DashMap::new)
}

fn obligations() -> &'static DashMap<String, String> {
    static OBLIGATIONS: OnceLock<DashMap<String, String>> = OnceLock::new();
    OBLIGATIONS.get_or_init(DashMap::new)
}

fn measurements() -> &'static DashMap<String, Duration> {
    static MEASURED: OnceLock<DashMap<String, Duration>> = OnceLock::new();
    MEASURED.get_or_init(DashMap::new)
}

fn lane_key(repo: &str, lane: Lane) -> (String, Lane) {
    let path = Path::new(repo);
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    (canonical.to_string_lossy().into_owned(), lane)
}

pub(super) fn name_obligation(ledger: &str, obligation: String) {
    obligations().insert(ledger.to_string(), obligation);
}

fn obligation_of(ledger: &str) -> String {
    obligations()
        .get(ledger)
        .map_or_else(|| ledger.to_string(), |named| named.clone())
}

pub(super) fn enlist(receipt: i64, enlisted: &Enlisted<'_>, owner: Owner) {
    entries().insert(
        receipt,
        Entry {
            lane: lane_key(enlisted.repo, enlisted.lane),
            obligation: obligation_of(enlisted.ledger),
            seat: enlisted.seat.to_string(),
            state: enlisted.state.to_string(),
            executing: false,
            owner,
        },
    );
}

pub(super) fn leave(receipt: Option<i64>) {
    if let Some(receipt) = receipt {
        entries().remove(&receipt);
    }
}

pub(super) fn executing(context: &Context) {
    let Some(receipt) = context.receipt_id else {
        return;
    };
    if let Some(mut entry) = entries().get_mut(&receipt) {
        entry.executing = true;
    }
    let Ok(identity) = context.authority.durable() else {
        return;
    };
    context.tracker.spawn(async move {
        if let Err(err) = identity.persist.mark_tree_check_executing(receipt).await {
            tracing::warn!(receipt, %err, "fleet: execution start was not recorded");
        }
    });
}

pub(super) fn withdraw(receipt: i64, seat: &str) -> Result<String, String> {
    let entry = entries().get(&receipt).ok_or_else(|| {
        format!("receipt {receipt} is not queued or running in this process; nothing was withdrawn")
    })?;
    if entry.seat != seat {
        return Err(format!(
            "receipt {receipt} was asked by {}, not {seat}; only its asker can withdraw it",
            entry.seat
        ));
    }
    let stage = if entry.executing { "running" } else { "queued" };
    entry.owner.cancel(format!("withdrawn by its asker {seat}"));
    Ok(format!(
        "receipt {receipt} was {stage} and is withdrawn; its result is recorded as cancelled"
    ))
}

pub(super) fn supersede(ledger: &str, state: &str) -> Vec<i64> {
    let obligation = obligation_of(ledger);
    let mut superseded = Vec::new();
    for entry in entries().iter() {
        if entry.obligation == obligation && entry.state != state && !entry.executing {
            entry.owner.cancel(format!(
                "superseded by a newer request for the same obligation at {state}"
            ));
            superseded.push(*entry.key());
        }
    }
    superseded
}

pub(super) fn depth(repo: &str, lane: Lane) -> Depth {
    let key = lane_key(repo, lane);
    let (mut waiting, mut executing) = (0, 0);
    for entry in entries().iter().filter(|entry| entry.lane == key) {
        if entry.owner.cancelled().is_some() {
            continue;
        }
        if entry.executing {
            executing += 1;
        } else {
            waiting += 1;
        }
    }
    Depth {
        waiting,
        executing,
        slots: lane.slots(),
    }
}

pub(super) fn expected_wait(depth: Depth, measured: Duration) -> Duration {
    let ahead = depth.waiting + depth.executing;
    let rounds = u32::try_from(ahead / depth.slots.max(1)).unwrap_or(u32::MAX);
    measured.saturating_mul(rounds)
}

pub(super) fn admit(
    depth: Depth,
    measured: Option<Duration>,
    patience: Duration,
) -> Result<(), String> {
    let Some(measured) = measured else {
        return Ok(());
    };
    let expected = expected_wait(depth, measured);
    if expected <= patience {
        return Ok(());
    }
    Err(format!(
        "queue is {} deep ({} running, {} waiting), expected wait {}s, patience {}s; this request was not queued. Ask again when the queue is shorter, or raise the operation's timeout_seconds",
        depth.waiting + depth.executing,
        depth.executing,
        depth.waiting,
        expected.as_secs(),
        patience.as_secs()
    ))
}

pub(super) fn patience_from(measured: Option<Duration>) -> Duration {
    measured.map_or(FALLBACK_PATIENCE, |measured| {
        measured
            .saturating_mul(PATIENCE_PER_RUN)
            .clamp(PATIENCE_FLOOR, PATIENCE_CEILING)
    })
}

pub(super) fn measured(command: &str) -> Option<Duration> {
    measurements().get(command.trim()).map(|found| *found)
}

pub(super) async fn measure(persist: &PersistHandle, command: &str) -> Option<Duration> {
    match persist.measured_tree_check_seconds(command).await {
        Ok(Some(seconds)) if seconds.is_finite() && seconds > 0.0 => {
            let duration = Duration::from_secs_f64(seconds);
            measurements().insert(command.trim().to_string(), duration);
            Some(duration)
        }
        Ok(_) => measured(command),
        Err(err) => {
            tracing::debug!(%err, "fleet: measured duration could not be read");
            measured(command)
        }
    }
}

#[cfg(test)]
#[path = "managed_queue_tests.rs"]
mod tests;
