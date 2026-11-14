use std::time::Duration;

use tokio::time::Instant;

use super::{Action, MAX_GAP, Wake};

#[test]
fn the_first_tick_runs_a_pass() {
    let mut wake = Wake::default();
    assert!(wake.due(None, Instant::now()));
}

#[test]
fn a_new_watch_run_wakes_the_runner_and_the_same_run_does_not() {
    let mut wake = Wake::default();
    let start = Instant::now();
    assert!(wake.due(Some(4), start));
    assert!(!wake.due(Some(4), start + Duration::from_secs(15)));
    assert!(wake.due(Some(5), start + Duration::from_secs(30)));
}

#[test]
fn an_idle_runner_passes_again_after_the_longest_gap() {
    let mut wake = Wake::default();
    let start = Instant::now();
    assert!(wake.due(Some(4), start));
    assert!(wake.due(Some(4), start + MAX_GAP));
}

#[test]
fn every_action_code_round_trips_and_only_reversible_ones_have_an_undo() {
    for action in Action::ALL {
        assert_eq!(Action::from_code(action.code()), Some(action));
    }
    let undoable: Vec<&str> = Action::ALL
        .into_iter()
        .filter(|a| a.can_undo())
        .map(Action::code)
        .collect();
    assert_eq!(undoable, ["set_setting", "disable_filter_rule"]);
    assert_eq!(Action::from_code("restart_proxy"), None);
}
