#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use serde_json::json;

use super::{FakeHerdr, envelope, pane_info};
use crate::launch::backend::TerminalBackend;
use crate::launch::herdr::HerdrBackend;

fn created(workspace: &str, pane: &str) -> String {
    envelope(json!({
        "root_pane": pane_info(workspace, &format!("{workspace}:t1"), pane, "/tmp/scratch"),
        "tab": {"tab_id": format!("{workspace}:t1"), "workspace_id": workspace},
        "type": "workspace_created",
        "workspace": {"workspace_id": workspace, "label": "mike-validate"}
    }))
}

#[tokio::test]
async fn validate_full_round_trip_passes_and_closes_the_workspace() {
    let fake = FakeHerdr::new();
    fake.answer("workspace create", &created("w9", "w9:p1"))
        .answer("pane run", "")
        .answer("pane wait-output", &envelope(json!({"matched_line": "mike-validate-ok", "pane_id": "w9:p1", "type": "output_matched"})))
        .answer("agent list", &envelope(json!({"agents": [], "type": "agent_list"})))
        .answer("workspace close", &envelope(json!({"type": "ok"})));
    let report = HerdrBackend::with_binary(fake.bin()).validate().await;
    assert!(report.passed, "{report:?}");
    let names: Vec<&str> = report.checks.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "workspace create",
            "pane run",
            "pane output",
            "agent list",
            "workspace close"
        ]
    );
    assert!(report.finished_at >= report.started_at);
    let calls = fake.calls();
    assert_eq!(calls[0][..2], ["workspace", "create"]);
    assert!(
        calls[0].contains(&"--label".to_string())
            && calls[0].contains(&"mike-validate".to_string())
    );
    assert!(calls[0].contains(&"--no-focus".to_string()));
    assert_eq!(
        calls[1],
        vec!["pane", "run", "w9:p1", "printf 'mike-%s\\n' validate-ok"]
    );
    assert_eq!(
        calls[2][..5],
        [
            "pane",
            "wait-output",
            "w9:p1",
            "--match",
            "mike-validate-ok"
        ]
    );
    assert_eq!(calls[4], vec!["workspace", "close", "w9"]);
}

#[tokio::test]
async fn validate_failed_pane_read_fails_report_but_still_closes() {
    let fake = FakeHerdr::new();
    fake.answer("workspace create", &created("w9", "w9:p1"))
        .answer("pane run", "")
        .error("pane wait-output", "timeout", "no match within 10000ms")
        .answer(
            "agent list",
            &envelope(json!({"agents": [], "type": "agent_list"})),
        )
        .answer("workspace close", &envelope(json!({"type": "ok"})));
    let report = HerdrBackend::with_binary(fake.bin()).validate().await;
    assert!(!report.passed);
    let output = report
        .checks
        .iter()
        .find(|c| c.name == "pane output")
        .unwrap_or_else(|| panic!("{report:?}"));
    assert!(!output.passed);
    assert!(output.detail.contains("timeout"));
    assert!(output.remedy.is_some());
    let close = report
        .checks
        .iter()
        .find(|c| c.name == "workspace close")
        .unwrap_or_else(|| panic!("{report:?}"));
    assert!(close.passed, "{report:?}");
    assert_eq!(
        fake.calls().last().unwrap(),
        &vec!["workspace", "close", "w9"]
    );
}

#[tokio::test]
async fn validate_create_failure_stops_early_with_one_failed_check() {
    let fake = FakeHerdr::new();
    fake.error("workspace create", "connect_failed", "no server");
    let report = HerdrBackend::with_binary(fake.bin()).validate().await;
    assert!(!report.passed);
    assert_eq!(report.checks.len(), 1);
    assert!(report.checks[0].detail.contains("connect_failed"));
    assert_eq!(fake.calls().len(), 1);
}

#[tokio::test]
async fn validate_marker_missing_from_matched_line_fails_output_check() {
    let fake = FakeHerdr::new();
    fake.answer("workspace create", &created("w9", "w9:p1"))
        .answer("pane run", "")
        .answer(
            "pane wait-output",
            &envelope(json!({"matched_line": "something else", "type": "output_matched"})),
        )
        .answer("agent list", &envelope(json!({"type": "agent_list"})))
        .error("workspace close", "workspace_not_found", "gone");
    let report = HerdrBackend::with_binary(fake.bin()).validate().await;
    assert!(!report.passed);
    let failed: Vec<&str> = report
        .checks
        .iter()
        .filter(|c| !c.passed)
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(failed, ["pane output", "agent list", "workspace close"]);
}
