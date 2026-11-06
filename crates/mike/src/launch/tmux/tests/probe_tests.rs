#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

use super::FakeTmux;
use crate::launch::backend::TerminalBackend;
use crate::launch::tmux::TmuxBackend;

#[tokio::test]
async fn probe_new_enough_version_is_available() {
    let fake = FakeTmux::new();
    fake.answer("-V", "tmux 3.7c\n");
    let backend = TmuxBackend::with_binary(fake.bin());
    let report = backend.probe().await;
    assert!(report.available, "{report:?}");
    assert_eq!(report.version.as_deref(), Some("3.7"));
    assert!(report.checks.iter().all(|c| c.passed));
}

#[tokio::test]
async fn probe_old_version_is_unavailable_with_upgrade_remedy() {
    let fake = FakeTmux::new();
    fake.answer("-V", "tmux 3.1\n");
    let backend = TmuxBackend::with_binary(fake.bin());
    let report = backend.probe().await;
    assert!(!report.available, "{report:?}");
    let version_check = report
        .checks
        .iter()
        .find(|c| c.name == "tmux version")
        .unwrap();
    assert!(!version_check.passed);
    assert!(version_check.remedy.as_deref().unwrap().contains("upgrade"));
}

#[tokio::test]
async fn probe_missing_binary_is_unavailable_with_install_remedy() {
    let backend = TmuxBackend::with_binary("/nonexistent/tmux-does-not-exist");
    let report = backend.probe().await;
    assert!(!report.available, "{report:?}");
    assert_eq!(report.checks.len(), 1);
    assert!(
        report.checks[0]
            .remedy
            .as_deref()
            .unwrap()
            .contains("install tmux")
    );
}
