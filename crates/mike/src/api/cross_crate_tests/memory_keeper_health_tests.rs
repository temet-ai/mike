//! Moved from crates/mike/src/memory_keeper/health_tests.rs by the crate split: it reaches the web
//! crate, so it is tested from there.

use chrono::Utc;

use crate::api::handlers::CapabilityState;
use crate::memory_keeper::health::{KeeperHealth, LastPass, capability_row};

fn running(last: Option<LastPass>) -> KeeperHealth {
    KeeperHealth {
        running: true,
        last,
    }
}

#[test]
fn capability_row_with_toggle_off_is_off_with_a_fix() {
    let row = capability_row(true, false, &running(None));
    assert_eq!(row.state, CapabilityState::Off);
    assert!(row.fix.contains("/settings"));
}

#[test]
fn capability_row_without_persistence_is_off() {
    let row = capability_row(false, true, &running(None));
    assert_eq!(row.state, CapabilityState::Off);
}

#[test]
fn capability_row_with_toggle_on_and_no_worker_is_inert() {
    let row = capability_row(true, true, &KeeperHealth::default());
    assert_eq!(row.state, CapabilityState::Inert);
    assert!(row.fix.contains("Restart"));
}

#[test]
fn capability_row_after_failed_build_is_degraded() {
    let failed = LastPass::Failed {
        at: Utc::now(),
        error: "pool timed out".to_owned(),
    };
    let row = capability_row(true, true, &running(Some(failed)));
    assert_eq!(row.state, CapabilityState::Degraded);
    assert!(row.detail.contains("pool timed out"));
}

#[test]
fn capability_row_when_lease_held_elsewhere_is_inert() {
    let row = capability_row(
        true,
        true,
        &running(Some(LastPass::NotLeader { at: Utc::now() })),
    );
    assert_eq!(row.state, CapabilityState::Inert);
    assert!(row.detail.contains("lease"));
}

#[test]
fn capability_row_after_built_pass_is_active() {
    let built = LastPass::Built {
        at: Utc::now(),
        rebuilt: 3,
        written: 1,
    };
    let row = capability_row(true, true, &running(Some(built)));
    assert_eq!(row.state, CapabilityState::Active);
    assert!(row.detail.contains("rebuilt 3"));
}
