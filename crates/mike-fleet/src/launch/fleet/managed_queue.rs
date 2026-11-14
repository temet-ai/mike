use crate::store::persistence::fleet_repo::FleetPersist as _;
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
pub const RESERVED_FOR_CHECK: &str = "#reserved-for-the-commit-check";
pub const QUEUE_WAIT: Duration = PATIENCE_CEILING;

pub fn waiting_limit(declared_timeout: Option<Duration>) -> Option<Duration> {
    declared_timeout.map(|declared| declared.max(QUEUE_WAIT))
}

pub fn reserve_for_commit_check(
    ledger: &str,
    operation: Option<super::operation::Operation>,
) -> Option<super::operation::Operation> {
    if ledger.contains('#') {
        return operation;
    }
    operation.map(|mut operation| {
        for claim in &mut operation.resources {
            *claim = reserved(claim);
        }
        operation
    })
}

fn reserved(claim: &super::operation::ResourceClaim) -> super::operation::ResourceClaim {
    use super::operation::{CountedResource, ResourceClaim};
    let name = format!("{}{RESERVED_FOR_CHECK}", claim.name().trim());
    match claim {
        ResourceClaim::Name(_) => ResourceClaim::Name(name),
        ResourceClaim::Counted(counted) => ResourceClaim::Counted(CountedResource {
            name,
            capacity: counted.capacity,
        }),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lane {
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

pub struct Enlisted<'a> {
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
pub struct Depth {
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

pub fn name_obligation(ledger: &str, obligation: String) {
    obligations().insert(ledger.to_string(), obligation);
}

fn obligation_of(ledger: &str) -> String {
    obligations()
        .get(ledger)
        .map_or_else(|| ledger.to_string(), |named| named.clone())
}

pub fn enlist(receipt: i64, enlisted: &Enlisted<'_>, owner: Owner) {
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

pub fn leave(receipt: Option<i64>) {
    if let Some(receipt) = receipt {
        entries().remove(&receipt);
    }
}

pub fn executing(context: &Context) {
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
        if let Err(err) = identity
            .persist
            .fleet()
            .mark_tree_check_executing(receipt)
            .await
        {
            tracing::warn!(receipt, error = %err, "fleet: execution start was not recorded");
        }
    });
}

pub fn withdraw(receipt: i64, seat: &str) -> Result<String, String> {
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

pub fn supersede(ledger: &str, state: &str) -> Vec<i64> {
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

pub fn depth(repo: &str, lane: Lane) -> Depth {
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

pub fn expected_wait(depth: Depth, measured: Duration) -> Duration {
    let ahead = depth.waiting + depth.executing;
    let rounds = u32::try_from(ahead / depth.slots.max(1)).unwrap_or(u32::MAX);
    measured.saturating_mul(rounds)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub ahead_running: usize,
    pub ahead_waiting: usize,
    pub expected_wait: Option<Duration>,
}

impl Place {
    pub fn describe(&self) -> String {
        let wait = self.expected_wait.map_or_else(String::new, |wait| {
            format!(", expected wait about {} s", wait.as_secs())
        });
        format!(
            "place {} in line ({} running and {} waiting ahead){wait}",
            self.ahead_waiting + 1,
            self.ahead_running,
            self.ahead_waiting
        )
    }
}

pub fn place(receipt: i64, measured: Option<Duration>) -> Option<Place> {
    let key = entries().get(&receipt).map(|entry| entry.lane.clone())?;
    let (mut ahead_running, mut ahead_waiting) = (0, 0);
    for entry in entries()
        .iter()
        .filter(|entry| entry.lane == key && *entry.key() != receipt)
    {
        if entry.owner.cancelled().is_some() {
            continue;
        }
        if entry.executing {
            ahead_running += 1;
        } else if *entry.key() < receipt {
            ahead_waiting += 1;
        }
    }
    let depth = Depth {
        waiting: ahead_waiting,
        executing: ahead_running,
        slots: key.1.slots(),
    };
    Some(Place {
        ahead_running,
        ahead_waiting,
        expected_wait: measured.map(|measured| expected_wait(depth, measured)),
    })
}

pub fn admit(
    depth: Depth,
    measured: Option<Duration>,
    patience: Option<Duration>,
) -> Result<(), String> {
    let (Some(measured), Some(patience)) = (measured, patience) else {
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

pub fn patience_from(measured: Option<Duration>) -> Duration {
    measured.map_or(FALLBACK_PATIENCE, |measured| {
        measured
            .saturating_mul(PATIENCE_PER_RUN)
            .clamp(PATIENCE_FLOOR, PATIENCE_CEILING)
    })
}

#[cfg(test)]
pub(super) fn remember_measurement(command: &str, run: Duration) {
    measurements().insert(command.trim().to_string(), run);
}

pub fn measured(command: &str) -> Option<Duration> {
    measurements().get(command.trim()).map(|found| *found)
}

pub async fn measure(persist: &PersistHandle, command: &str) -> Option<Duration> {
    match persist.fleet().measured_tree_check_seconds(command).await {
        Ok(Some(seconds)) if seconds.is_finite() && seconds > 0.0 => {
            let duration = Duration::from_secs_f64(seconds);
            measurements().insert(command.trim().to_string(), duration);
            Some(duration)
        }
        Ok(_) => measured(command),
        Err(err) => {
            tracing::debug!(error = %err, "fleet: measured duration could not be read");
            measured(command)
        }
    }
}

#[cfg(test)]
#[path = "managed_queue_tests.rs"]
mod tests;
