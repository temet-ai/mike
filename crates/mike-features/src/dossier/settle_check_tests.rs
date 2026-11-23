use std::time::Duration;

use tokio::time::Instant;

use super::{SETTLE_MARGIN, SettleCheck};

const SETTLE: Duration = Duration::from_secs(1_800);

#[test]
fn nothing_is_due_before_a_build() {
    let mut check = SettleCheck::default();
    assert_eq!(check.due(), None);
    assert!(!check.take_due(Instant::now(), SETTLE));
}

#[test]
fn a_build_makes_a_check_due_once_its_sessions_settle() {
    let start = Instant::now();
    let mut check = SettleCheck::default();
    check.note_build(start, SETTLE);
    assert_eq!(check.due(), Some(start + SETTLE + SETTLE_MARGIN));
    assert!(!check.take_due(start + SETTLE, SETTLE));
    assert!(check.take_due(start + SETTLE + SETTLE_MARGIN, SETTLE));
    assert_eq!(check.due(), None);
}

#[test]
fn later_builds_keep_the_first_due_and_schedule_one_more() {
    let start = Instant::now();
    let later = start + Duration::from_secs(600);
    let mut check = SettleCheck::default();
    check.note_build(start, SETTLE);
    check.note_build(later, SETTLE);
    assert_eq!(check.due(), Some(start + SETTLE + SETTLE_MARGIN));
    assert!(check.take_due(start + SETTLE + SETTLE_MARGIN, SETTLE));
    assert_eq!(check.due(), Some(later + SETTLE + SETTLE_MARGIN));
    assert!(check.take_due(later + SETTLE + SETTLE_MARGIN, SETTLE));
    assert_eq!(check.due(), None);
}
