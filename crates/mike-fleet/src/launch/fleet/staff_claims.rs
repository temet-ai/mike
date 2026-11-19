use std::collections::{BTreeMap, BTreeSet};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use uuid::Uuid;

pub const CLAIM_PATIENCE: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Copy)]
struct Claim {
    session: Uuid,
    since: Instant,
}

type Key = (String, String);

static CLAIMS: LazyLock<Mutex<BTreeMap<Key, Claim>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

fn claims() -> MutexGuard<'static, BTreeMap<Key, Claim>> {
    CLAIMS.lock().unwrap_or_else(PoisonError::into_inner)
}

fn key(run: &str, task: &str) -> Key {
    (run.to_string(), task.to_string())
}

pub fn begin(run: &str, task: &str, session: Uuid) -> bool {
    let mut held = claims();
    if held.contains_key(&key(run, task)) {
        return false;
    }
    held.insert(
        key(run, task),
        Claim {
            session,
            since: Instant::now(),
        },
    );
    true
}

pub fn end(run: &str, task: &str) {
    claims().remove(&key(run, task));
}

pub fn end_session(run: &str, task: &str, session: Uuid) {
    let mut held = claims();
    if held
        .get(&key(run, task))
        .is_some_and(|claim| claim.session == session)
    {
        held.remove(&key(run, task));
    }
}

pub fn held_by(run: &str) -> BTreeSet<String> {
    claims()
        .keys()
        .filter(|(held_run, _)| held_run == run)
        .map(|(_, task)| task.clone())
        .collect()
}

pub fn settle(run: &str, queued: &BTreeSet<String>, now: Instant) {
    claims().retain(|(held_run, task), claim| {
        held_run != run
            || (queued.contains(task)
                && now.saturating_duration_since(claim.since) < CLAIM_PATIENCE)
    });
}

static WAITING: LazyLock<Mutex<BTreeMap<String, BTreeMap<String, String>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

pub fn set_waiting(run: &str, reasons: BTreeMap<String, String>) {
    WAITING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(run.to_string(), reasons);
}

pub fn waiting(run: &str) -> BTreeMap<String, String> {
    WAITING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(run)
        .cloned()
        .unwrap_or_default()
}

pub fn annotate(status: &mut super::status::FleetStatus) {
    let Some(run) = status.run_id.as_deref() else {
        return;
    };
    let reasons = waiting(run);
    for task in &mut status.tasks {
        if task.state != super::staff::QUEUED {
            continue;
        }
        if let Some(why) = reasons.get(&task.id) {
            task.state = format!("{}: {why}", super::staff::QUEUED);
        }
    }
}

#[cfg(test)]
#[path = "staff_claims_tests.rs"]
mod tests;
