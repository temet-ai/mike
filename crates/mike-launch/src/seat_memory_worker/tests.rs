use super::{BACKSTOP, RefreshMark};
use std::time::Duration;
use tokio::time::Instant;

#[test]
fn refresh_mark_never_refreshed_is_due() {
    assert!(RefreshMark::default().due(0, Instant::now()));
}

#[test]
fn refresh_mark_same_generation_within_backstop_is_not_due() {
    let now = Instant::now();
    let mut mark = RefreshMark::default();
    mark.record(7, now);
    assert!(!mark.due(7, now + Duration::from_secs(60)));
}

#[test]
fn refresh_mark_moved_generation_is_due() {
    let now = Instant::now();
    let mut mark = RefreshMark::default();
    mark.record(7, now);
    assert!(mark.due(8, now + Duration::from_secs(1)));
}

#[test]
fn refresh_mark_same_generation_after_backstop_is_due() {
    let now = Instant::now();
    let mut mark = RefreshMark::default();
    mark.record(7, now);
    assert!(mark.due(7, now + BACKSTOP));
}
