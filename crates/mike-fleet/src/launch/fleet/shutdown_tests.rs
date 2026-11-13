use super::{ManagedStop, Sweep, stop_requested_event, sweep};
use crate::launch::fleet::events::{EventPayload, NewFleetEvent, StopRequested};

#[test]
fn a_stop_request_records_stop_requested_for_its_run() {
    assert_eq!(
        stop_requested_event("run-1"),
        NewFleetEvent {
            run_id: "run-1".into(),
            task: None,
            session_uuid: None,
            payload: EventPayload::StopRequested(StopRequested {
                by: "mike fleet stop".into()
            }),
        }
    );
}

#[test]
fn a_seat_whose_process_is_gone_is_exited() {
    let found = sweep(&[(1, Some(10)), (2, Some(20))], |pid| pid == 20);
    assert_eq!(
        found,
        Sweep {
            exited: vec![1],
            live: 1
        }
    );
}

#[test]
fn a_seat_without_a_pid_counts_as_live() {
    let found = sweep(&[(1, None)], |_| false);
    assert_eq!(
        found,
        Sweep {
            exited: vec![],
            live: 1
        }
    );
}

#[test]
fn an_invalid_pid_does_not_prove_a_root_exited() {
    let found = sweep(&[(1, Some(0)), (2, Some(-1))], |_| false);
    assert_eq!(
        found,
        Sweep {
            exited: vec![],
            live: 2
        }
    );
}

#[test]
fn no_seats_leaves_nothing_live() {
    assert_eq!(sweep(&[], |_| true), Sweep::default());
}

#[test]
fn either_live_owner_or_durable_receipt_blocks_managed_completion() {
    let mut report = ManagedStop::default();
    assert!(report.settled());
    report.pending_receipts.push(8);
    assert!(!report.settled());
    report.pending_receipts.clear();
    report.pending_owners.push("interrupted owner".into());
    assert!(!report.settled());
}

#[test]
fn empty_managed_stop_always_reports_platform_containment() -> anyhow::Result<()> {
    let report = ManagedStop::default();
    let expected = crate::launch::fleet::managed_containment_capability();
    assert_eq!(report.containment, expected);
    let encoded = serde_json::to_value(report)?;
    assert_eq!(encoded["containment"], expected);
    Ok(())
}
