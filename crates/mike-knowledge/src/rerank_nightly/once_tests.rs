use super::once::{finished, mark_finished};
use super::window_start;
use chrono::{TimeZone, Utc};

#[test]
fn the_per_thread_work_runs_once_per_quiet_window() {
    let tonight = Utc.with_ymd_and_hms(2031, 3, 4, 23, 10, 0).unwrap();
    let later = Utc.with_ymd_and_hms(2031, 3, 5, 1, 40, 0).unwrap();
    let next_night = Utc.with_ymd_and_hms(2031, 3, 5, 23, 10, 0).unwrap();
    let window = window_start(tonight, 22);
    assert!(!finished(window));
    mark_finished(window);
    assert!(finished(window_start(later, 22)), "a later tick in the same window skips it");
    assert!(!finished(window_start(next_night, 22)), "the next window runs it again");
}
