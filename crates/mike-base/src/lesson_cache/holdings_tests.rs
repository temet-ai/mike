use super::{HeldRow, SessionHoldings};

#[test]
fn hold_same_revision_twice_reports_it_new_once() {
    let holdings = SessionHoldings::default();
    assert_eq!(holdings.hold("s1", &[(7, 1)]), vec![(7, 1)]);
    assert!(holdings.hold("s1", &[(7, 1)]).is_empty());
    assert!(holdings.holds("s1", 7, 1));
    assert!(!holdings.holds("s1", 7, 2));
    assert!(!holdings.holds("s2", 7, 1));
}

#[test]
fn hold_newer_revision_replaces_the_held_one() {
    let holdings = SessionHoldings::default();
    holdings.hold("s1", &[(7, 1)]);
    assert_eq!(holdings.hold("s1", &[(7, 2)]), vec![(7, 2)]);
    assert!(holdings.holds("s1", 7, 2));
    assert!(!holdings.holds("s1", 7, 1));
}

#[test]
fn forget_session_releases_only_that_session() {
    let holdings = SessionHoldings::default();
    holdings.hold("s1", &[(7, 1)]);
    holdings.hold("s2", &[(7, 1)]);
    holdings.forget("s1");
    assert!(!holdings.holds("s1", 7, 1));
    assert!(holdings.holds("s2", 7, 1));
}

#[test]
fn restore_after_restart_brings_back_clients_and_held_lessons() {
    let holdings = SessionHoldings::default();
    holdings.restore(
        vec!["client".to_owned()],
        vec![HeldRow {
            session: "s1".to_owned(),
            lesson_id: 9,
            revision: 3,
        }],
    );
    assert!(holdings.is_client("client"));
    assert!(!holdings.is_client("s1"));
    assert!(holdings.holds("s1", 9, 3));
    assert_eq!(holdings.held_ids("s1"), vec![9]);
}

#[test]
fn restore_keeps_a_revision_held_since_startup() {
    let holdings = SessionHoldings::default();
    holdings.hold("s1", &[(9, 4)]);
    holdings.restore(
        Vec::new(),
        vec![HeldRow {
            session: "s1".to_owned(),
            lesson_id: 9,
            revision: 3,
        }],
    );
    assert!(holdings.holds("s1", 9, 4));
}

#[test]
fn note_client_marks_the_session_while_another_reader_holds_the_lock() {
    let holdings = SessionHoldings::default();
    let reading = holdings.is_client("other");
    holdings.note_client("s1");
    assert!(!reading);
    assert!(holdings.is_client("s1"));
}

#[test]
fn note_declared_is_new_once_and_separate_from_the_lesson_route() {
    let holdings = SessionHoldings::default();
    assert!(holdings.note_declared("s1"));
    assert!(!holdings.note_declared("s1"));
    assert!(holdings.is_declared("s1"));
    assert!(!holdings.is_client("s1"));
    holdings.restore_declared(vec!["s2".to_owned()]);
    assert!(holdings.is_declared("s2"));
}
