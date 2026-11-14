use std::time::Duration;

use super::{CodeNote, CodeNotes, VIA_PUSH, VIA_REQUEST};
use uuid::Uuid;

fn note(id: i64) -> CodeNote {
    CodeNote {
        id,
        path: "a.rs".to_owned(),
        line: 1,
        rule_id: "r".to_owned(),
        message: "m".to_owned(),
        fix: None,
    }
}

#[test]
fn notes_are_taken_once_per_session_in_order_without_duplicates() {
    let notes = CodeNotes::default();
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    notes.post(a, vec![note(1), note(2), note(1)]);
    notes.post(a, vec![note(3)]);
    notes.post(b, vec![note(9)]);
    let first: Vec<i64> = notes.take(a, 2).iter().map(|n| n.id).collect();
    assert_eq!(first, vec![1, 2]);
    assert_eq!(notes.take(a, 3).len(), 1);
    assert!(notes.take(a, 3).is_empty());
    assert_eq!(notes.take(b, 3).len(), 1);
}

#[test]
fn delivered_ids_are_drained_once_with_the_capture_id() {
    let notes = CodeNotes::default();
    notes.mark_sent(&[4, 5], 11);
    assert!(notes.drain_delivered().is_empty());
    notes.bind_capture_id(12, 900);
    assert!(notes.drain_delivered().is_empty());
    notes.bind_capture_id(11, 797_027);
    let delivered = notes.drain_delivered();
    assert_eq!(delivered.len(), 2);
    assert!(
        delivered
            .iter()
            .all(|d| d.request_id == Some(797_027) && d.via == VIA_REQUEST)
    );
    assert!(notes.drain_delivered().is_empty());
}

#[test]
fn unread_notes_wait_before_they_are_offered_for_a_push() {
    let notes = CodeNotes::default();
    let session = Uuid::new_v4();
    notes.post(session, vec![note(1)]);
    assert!(notes.take_unread(Duration::from_secs(60), 3).is_empty());
    let offered = notes.take_unread(Duration::ZERO, 3);
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].0, session);
    assert!(notes.take(session, 3).is_empty());
}

#[test]
fn a_failed_push_leaves_the_note_for_the_request_but_never_offers_it_again() {
    let notes = CodeNotes::default();
    let session = Uuid::new_v4();
    notes.put_back_unpushable(session, vec![note(7)]);
    assert!(notes.take_unread(Duration::ZERO, 3).is_empty());
    assert_eq!(notes.take(session, 3).len(), 1);
}

#[test]
fn a_pushed_note_is_delivered_with_the_sessions_next_request() {
    let notes = CodeNotes::default();
    let (session, other) = (Uuid::new_v4(), Uuid::new_v4());
    notes.mark_pushed(session, &[3, 4]);
    notes.bind_session_request(other, 1);
    assert!(notes.drain_delivered().is_empty());
    notes.bind_session_request(session, 800_100);
    let delivered = notes.drain_delivered();
    assert_eq!(delivered.len(), 2);
    assert!(
        delivered
            .iter()
            .all(|d| d.request_id == Some(800_100) && d.via == VIA_PUSH)
    );
    notes.bind_session_request(session, 800_101);
    assert!(notes.drain_delivered().is_empty());
}
