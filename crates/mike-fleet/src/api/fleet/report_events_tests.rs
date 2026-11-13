use uuid::Uuid;

use super::{filed_event, verdict_events};
use crate::launch::fleet::events::{EventPayload, NewFleetEvent, ReportFiled, VerdictRecorded};

#[test]
fn a_filed_report_records_report_filed_for_its_task_and_seat() {
    let (session, note) = (Uuid::from_u128(4), Uuid::from_u128(9));
    assert_eq!(
        filed_event("run-1", "T12", session, note),
        NewFleetEvent {
            run_id: "run-1".into(),
            task: Some("T12".into()),
            session_uuid: Some(session),
            payload: EventPayload::ReportFiled(ReportFiled { note_uuid: note }),
        }
    );
}

#[test]
fn a_verdict_records_one_verdict_event_per_criterion_with_its_outcome() {
    let (acceptance, note) = (Uuid::from_u128(5), Uuid::from_u128(9));
    let criteria = ["C1".to_owned(), "C2".to_owned()];
    let events = verdict_events("run-1", "T12", acceptance, note, &criteria, false);
    assert!(events.iter().all(|e| e.session_uuid == Some(acceptance)));
    let outcomes: Vec<_> = events
        .iter()
        .filter_map(|e| match &e.payload {
            EventPayload::VerdictRecorded(VerdictRecorded {
                criterion, passed, ..
            }) => Some((criterion.clone(), *passed)),
            _ => None,
        })
        .collect();
    assert_eq!(
        outcomes,
        vec![("C1".to_owned(), false), ("C2".to_owned(), false)]
    );
    assert_eq!(events.len(), 2);
}
