use super::health::{Health, Waiting, capability_row, waiting_words};
use crate::web_base::capability::CapabilityState;
use chrono::Utc;

fn running() -> Health {
    Health {
        running: true,
        leader: true,
        ..Health::default()
    }
}

#[test]
fn off_without_persistence_or_the_toggle_and_inert_before_the_worker_starts() {
    let off = capability_row(false, true, &running(), None);
    assert_eq!(off.state, CapabilityState::Off);
    let toggle = capability_row(true, false, &running(), None);
    assert_eq!(toggle.state, CapabilityState::Off);
    assert!(toggle.fix.contains("/settings/workers"));
    let starting = capability_row(true, true, &Health::default(), None);
    assert_eq!(starting.state, CapabilityState::Inert);
}

#[test]
fn working_reports_the_queue_and_the_last_pass() {
    let health = Health {
        last_pass_at: Some(Utc::now()),
        last_summary: "items 3 done".to_owned(),
        ..running()
    };
    let waiting = Waiting {
        files: 14,
        other_items: 0,
        subjects: 3,
    };
    let row = capability_row(true, true, &health, Some(waiting));
    assert_eq!(row.state, CapabilityState::Active);
    assert_eq!(row.detail, "14 files waiting to be split · 3 subjects waiting to be judged");
    let idle = capability_row(true, true, &health, Some(Waiting::default()));
    assert_eq!(idle.detail, "Nothing waiting.");
}

#[test]
fn waiting_work_is_spelled_out_with_singular_and_plural_forms() {
    let one = Waiting {
        files: 1,
        other_items: 2,
        subjects: 1,
    };
    assert_eq!(
        waiting_words(one),
        "1 file waiting to be split · 2 lessons and session items waiting to be read · 1 subject waiting to be judged"
    );
}

#[test]
fn an_error_degrades_with_the_message_and_the_remedy() {
    let health = Health {
        error: Some((Utc::now(), "embedding timed out".to_owned())),
        ..running()
    };
    let row = capability_row(true, true, &health, None);
    assert_eq!(row.state, CapabilityState::Degraded);
    assert!(row.detail.contains("embedding timed out"));
    assert!(!row.fix.is_empty());
}

#[test]
fn a_worker_without_the_lease_is_inert() {
    let health = Health {
        running: true,
        leader: false,
        ..Health::default()
    };
    assert_eq!(capability_row(true, true, &health, None).state, CapabilityState::Inert);
}
