use crate::launch::backend::TerminalBackend;
use crate::launch::terminal::XdgTerminalBackend;

use super::{FakeTerminalHost, fast_timeouts};

fn backend(host: &FakeTerminalHost) -> XdgTerminalBackend {
    XdgTerminalBackend::with_binaries_and_timeouts(host.xdg_bin(), host.hypr_bin(), fast_timeouts())
}

#[tokio::test]
async fn validate_every_check_passes_on_a_healthy_host() {
    let host = FakeTerminalHost::new();
    let report = backend(&host).validate().await;
    assert!(report.passed, "{report:?}");
    let names: Vec<&str> = report.checks.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "window opens",
            "command ran",
            "window closes on its own",
            "forced close",
        ],
        "{report:?}"
    );
    assert!(report.finished_at >= report.started_at);
}

#[tokio::test]
async fn validate_hyprctl_never_reporting_a_window_fails_every_window_dependent_check() {
    let host = FakeTerminalHost::new();
    host.make_clients_report_no_windows();
    let report = backend(&host).validate().await;
    assert!(!report.passed, "{report:?}");
    let names: Vec<&str> = report.checks.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["window opens", "forced close"], "{report:?}");
    assert!(report.checks.iter().all(|c| !c.passed), "{report:?}");
}
