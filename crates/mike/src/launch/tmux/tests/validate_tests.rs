#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

use std::time::Duration;

use super::FakeTmux;
use crate::launch::backend::TerminalBackend;
use crate::launch::tmux::TmuxBackend;

#[tokio::test]
async fn validate_every_step_answering_is_passed() {
    let fake = FakeTmux::new();
    fake.answer("new-session", "");
    fake.answer("send-keys", "");
    fake.answer("capture-pane", "mike-tmux-validate-ok\n");
    fake.answer("list-panes", "%1\n");
    fake.answer("kill-session", "");
    let backend = TmuxBackend::with_binary(fake.bin());
    let report = backend.validate().await;
    assert!(report.passed, "{report:?}");
    assert_eq!(report.checks.len(), 5);
    assert!(report.checks.iter().all(|c| c.passed));
}

#[tokio::test]
async fn validate_new_session_failure_stops_before_kill_and_reports_one_check() {
    let fake = FakeTmux::new();
    fake.error("new-session", "no server running on /tmp/tmux-0/default");
    let backend = TmuxBackend::with_binary(fake.bin());
    let report = backend.validate().await;
    assert!(!report.passed);
    assert_eq!(report.checks.len(), 1);
    assert!(!report.checks[0].passed);
}

#[tokio::test]
async fn validate_marker_never_appears_fails_that_check_but_still_kills() {
    let fake = FakeTmux::new();
    fake.answer("new-session", "");
    fake.answer("send-keys", "");
    fake.answer("capture-pane", "\n");
    fake.answer("list-panes", "%1\n");
    fake.answer("kill-session", "");
    let backend =
        TmuxBackend::with_binary_and_capture_budget(fake.bin(), Duration::from_millis(200));
    let report = backend.validate().await;
    assert!(!report.passed);
    let capture = report
        .checks
        .iter()
        .find(|c| c.name == "capture-pane")
        .unwrap();
    assert!(!capture.passed);
    let kill = report
        .checks
        .iter()
        .find(|c| c.name == "kill-session")
        .unwrap();
    assert!(kill.passed, "kill-session must still run: {kill:?}");
}
