use super::render_stop;
use crate::launch::fleet::shutdown::{ManagedStop, SeatStop, StopReport};
use uuid::Uuid;

#[test]
fn stopping_report_shows_root_and_command_blockers_without_claiming_done() {
    let report = StopReport {
        fleet: Uuid::nil(),
        slug: "sample".into(),
        state: "stopping".into(),
        seats: vec![SeatStop {
            launch_id: 1,
            seat: Some("worker".into()),
            session: None,
            closed: true,
            process_exited: Some(false),
            note: Some("wait for root exit".into()),
        }],
        managed: ManagedStop {
            containment: "owned-group/test-fixture".into(),
            known_in_this_process: false,
            stop_requested: true,
            pending_owners: vec!["execution still recording".into()],
            pending_receipts: vec![7],
        },
        untracked_descendants: "untracked descendants remain unknown".into(),
        root_origin: "local-sysctl/session-run/macos-runtime-unvalidated".into(),
    };
    let text = render_stop(&report);
    assert!(text.starts_with("sample: stopping"));
    assert!(text.contains("pane close acknowledged; root process still present"));
    assert!(text.contains("execution still recording"));
    assert!(text.contains("seat root origin: local-sysctl/session-run/macos-runtime-unvalidated"));
    assert!(text.contains("managed containment: owned-group/test-fixture"));
    assert!(text.contains("unfinished command receipts: [7]"));
    assert!(text.contains("Retry fleet stop"));
    assert!(text.contains("untracked descendants remain unknown"));
    assert!(!text.contains("sample: done"));
}

#[test]
fn legacy_stop_report_missing_exit_proof_renders_unknown() -> anyhow::Result<()> {
    let report: StopReport = serde_json::from_value(serde_json::json!({
        "fleet": Uuid::nil(), "slug": "legacy", "state": "done",
        "seats": [{"seat": "worker", "session": null, "closed": true, "note": null}]
    }))?;
    let text = render_stop(&report);
    assert!(text.contains("root process exit unknown"));
    assert!(!text.contains("root process exited"));
    Ok(())
}
