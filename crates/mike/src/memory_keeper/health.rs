use std::sync::{Arc, LazyLock};

use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};

use crate::api::handlers::{CapabilityRow, CapabilityState};

const ID: &str = "memory_keeper";
const LABEL: &str = "Work thread cards";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LastPass {
    Built {
        at: DateTime<Utc>,
        rebuilt: usize,
        written: usize,
    },
    Failed {
        at: DateTime<Utc>,
        error: String,
    },
    NotLeader {
        at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct KeeperHealth {
    pub(crate) running: bool,
    pub(crate) last: Option<LastPass>,
}

static HEALTH: LazyLock<ArcSwap<KeeperHealth>> =
    LazyLock::new(|| ArcSwap::from_pointee(KeeperHealth::default()));

pub(crate) fn snapshot() -> KeeperHealth {
    (**HEALTH.load()).clone()
}

pub(crate) fn set_running(running: bool) {
    HEALTH.rcu(|current| {
        Arc::new(KeeperHealth {
            running,
            last: current.last.clone(),
        })
    });
}

pub(crate) fn record(pass: LastPass) {
    HEALTH.rcu(|current| {
        Arc::new(KeeperHealth {
            running: current.running,
            last: Some(pass.clone()),
        })
    });
}

fn row(state: CapabilityState, detail: String, fix: &str) -> CapabilityRow {
    CapabilityRow {
        id: ID,
        label: LABEL,
        state,
        detail,
        fix: fix.to_owned(),
    }
}

fn running_row(last: Option<&LastPass>) -> CapabilityRow {
    match last {
        None => row(
            CapabilityState::Active,
            "The card worker is running and has not finished its first pass yet".to_owned(),
            "",
        ),
        Some(LastPass::Built {
            at,
            rebuilt,
            written,
        }) => row(
            CapabilityState::Active,
            format!("Last pass at {at}: rebuilt {rebuilt} cards, {written} with new text"),
            "",
        ),
        Some(LastPass::Failed { at, error }) => row(
            CapabilityState::Degraded,
            format!("The last card build failed at {at}: {error}. Cards keep their previous text"),
            "Check the mike log for memory_keeper errors and the database connection",
        ),
        Some(LastPass::NotLeader { at }) => row(
            CapabilityState::Inert,
            format!(
                "Another machine holds the memory_keeper_cards lease and builds the cards (checked at {at})"
            ),
            "",
        ),
    }
}

pub(crate) fn capability_row(
    persistence: bool,
    enabled: bool,
    health: &KeeperHealth,
) -> CapabilityRow {
    if !persistence {
        return row(
            CapabilityState::Off,
            "Persistence is off, so there are no work threads to build cards for".to_owned(),
            "",
        );
    }
    if !enabled {
        return row(
            CapabilityState::Off,
            "The memory_keeper toggle is off, so no work thread cards are built".to_owned(),
            "Turn on Work thread cards at /settings, then restart mike",
        );
    }
    if !health.running {
        return row(
            CapabilityState::Inert,
            "The memory_keeper toggle is on but the card worker is not running; it starts only when the toggle is on at startup".to_owned(),
            "Restart mike",
        );
    }
    running_row(health.last.as_ref())
}

#[cfg(test)]
#[path = "health_tests.rs"]
mod health_tests;
