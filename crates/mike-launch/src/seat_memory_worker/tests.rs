use super::{BACKSTOP, MIN_INTERVAL, RefreshMark, Refreshed, SWEEP};
use crate::store::persistence::seat_memory::SeatMemoryMarks;
use std::time::Duration;
use tokio::time::Instant;

const MARKS: SeatMemoryMarks = SeatMemoryMarks {
    launch: 3,
    fleet_event: 5,
    turn_closed: chrono::DateTime::UNIX_EPOCH,
};

fn swept() -> Refreshed {
    Refreshed {
        marks: MARKS,
        swept: true,
    }
}

fn scoped(marks: SeatMemoryMarks) -> Refreshed {
    Refreshed {
        marks,
        swept: false,
    }
}

#[test]
fn refresh_mark_never_refreshed_is_due() {
    assert!(RefreshMark::default().due(0, Instant::now()));
}

#[test]
fn refresh_mark_same_generation_within_backstop_is_not_due() {
    let now = Instant::now();
    let mut mark = RefreshMark::default();
    mark.record(7, now, swept());
    assert!(!mark.due(7, now + Duration::from_secs(60)));
}

#[test]
fn refresh_mark_moved_generation_is_due_after_min_interval() {
    let now = Instant::now();
    let mut mark = RefreshMark::default();
    mark.record(7, now, swept());
    assert!(mark.due(8, now + MIN_INTERVAL));
}

#[test]
fn refresh_mark_moved_generation_within_min_interval_is_not_due() {
    let now = Instant::now();
    let mut mark = RefreshMark::default();
    mark.record(7, now, swept());
    assert!(!mark.due(8, now + Duration::from_secs(30)));
    assert!(!mark.due(9, now + MIN_INTERVAL - Duration::from_secs(1)));
}

#[test]
fn refresh_mark_same_generation_after_backstop_is_due() {
    let now = Instant::now();
    let mut mark = RefreshMark::default();
    mark.record(7, now, swept());
    assert!(mark.due(7, now + BACKSTOP));
}

#[test]
fn refresh_mark_never_refreshed_sweeps_every_project() {
    assert_eq!(RefreshMark::default().since(Instant::now()), None);
}

#[test]
fn refresh_mark_moved_generation_within_backstop_refreshes_only_changed_projects() {
    let now = Instant::now();
    let mut mark = RefreshMark::default();
    mark.record(7, now, swept());
    assert_eq!(mark.since(now + Duration::from_secs(30)), Some(MARKS));
}

#[test]
fn refresh_mark_scoped_refresh_advances_marks_but_not_the_sweep_clock() {
    let now = Instant::now();
    let later = SeatMemoryMarks {
        launch: 4,
        fleet_event: 9,
        turn_closed: chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::seconds(12),
    };
    let mut mark = RefreshMark::default();
    mark.record(7, now, swept());
    mark.record(8, now + Duration::from_secs(30), scoped(later));
    assert_eq!(mark.since(now + Duration::from_secs(60)), Some(later));
    assert_eq!(mark.since(now + BACKSTOP), Some(later));
    assert_eq!(mark.since(now + SWEEP), None);
}

#[test]
fn refresh_mark_backstop_refresh_is_scoped_until_the_daily_sweep() {
    let now = Instant::now();
    let mut mark = RefreshMark::default();
    mark.record(7, now, swept());
    assert!(mark.due(7, now + BACKSTOP));
    assert_eq!(mark.since(now + BACKSTOP), Some(MARKS));
    mark.record(7, now + BACKSTOP, scoped(MARKS));
    assert!(!mark.due(7, now + BACKSTOP + Duration::from_secs(60)));
    assert_eq!(mark.since(now + SWEEP), None);
}
