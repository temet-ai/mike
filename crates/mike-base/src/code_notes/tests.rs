use std::time::Duration;

use super::{CodeNote, CodeNotes, VIA_REQUEST};
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

fn now(notes: &CodeNotes, session: Uuid) -> Vec<i64> {
    notes
        .take_settled(session, Duration::ZERO, Duration::ZERO)
        .iter()
        .map(|n| n.id)
        .collect()
}

#[test]
fn notes_are_taken_together_once_per_session_without_duplicates() {
    let notes = CodeNotes::default();
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    notes.post(a, vec![note(1), note(2), note(1)]);
    notes.post(a, vec![note(3)]);
    notes.post(b, vec![note(9)]);
    assert_eq!(now(&notes, a), vec![1, 2, 3]);
    assert!(now(&notes, a).is_empty());
    assert_eq!(now(&notes, b), vec![9]);
}

#[test]
fn notes_wait_until_edits_settle_and_come_no_closer_than_the_gap() {
    let notes = CodeNotes::default();
    let session = Uuid::new_v4();
    let minute = Duration::from_secs(60);
    notes.post(session, vec![note(1)]);
    assert!(notes.take_settled(session, minute, minute).is_empty());
    assert_eq!(notes.take_settled(session, Duration::ZERO, minute).len(), 1);
    notes.post(session, vec![note(2)]);
    assert!(
        notes
            .take_settled(session, Duration::ZERO, minute)
            .is_empty()
    );
    assert_eq!(now(&notes, session), vec![2]);
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
fn a_withdrawn_note_is_not_handed_out() {
    let notes = CodeNotes::default();
    let session = Uuid::new_v4();
    notes.post(session, vec![note(1), note(2)]);
    notes.withdraw(&[1]);
    assert_eq!(now(&notes, session), vec![2]);
}
