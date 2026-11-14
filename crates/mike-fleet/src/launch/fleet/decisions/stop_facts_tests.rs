use chrono::{Duration, Utc};

use super::*;

fn check(passed: bool, offset_seconds: i64) -> LastCheck {
    LastCheck {
        passed,
        finished_at: Some(Utc::now() + Duration::seconds(offset_seconds)),
    }
}

#[test]
fn passed_after_a_passing_check_newer_than_the_change_is_true() {
    let change = Utc::now();
    assert!(passed_after(Some(&check(true, 5)), Some(change)));
}

#[test]
fn passed_after_a_passing_check_older_than_the_change_is_false() {
    let change = Utc::now();
    assert!(!passed_after(Some(&check(true, -60)), Some(change)));
}

#[test]
fn passed_after_a_failing_check_is_false() {
    assert!(!passed_after(Some(&check(false, 5)), Some(Utc::now())));
}

#[test]
fn passed_after_no_check_is_false() {
    assert!(!passed_after(None, Some(Utc::now())));
}

#[test]
fn unchecked_changes_needs_uncommitted_files_and_no_passing_check() {
    let mut facts = StopFacts {
        seat: "worker-T1".into(),
        task: "T1".into(),
        uncommitted: vec!["src/a.rs".into()],
        last_change_at: None,
        check_passed_after_change: false,
        last_check: None,
        open_asks: Vec::new(),
        check_running: false,
        done_when: None,
    };
    assert!(facts.unchecked_changes());
    facts.check_passed_after_change = true;
    assert!(!facts.unchecked_changes());
    facts.check_passed_after_change = false;
    facts.uncommitted.clear();
    assert!(!facts.unchecked_changes());
}
