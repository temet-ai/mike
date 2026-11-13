use crate::launch::BackendId;
use crate::launch::backend::TerminalBackend;
use crate::launch::terminal::XdgTerminalBackend;

use super::FakeTerminalHost;

fn backend(host: &FakeTerminalHost) -> XdgTerminalBackend {
    XdgTerminalBackend::with_binaries(host.xdg_bin(), host.hypr_bin())
}

#[tokio::test]
async fn probe_every_check_passes_on_a_healthy_host() {
    if std::env::var("WAYLAND_DISPLAY")
        .map(|v| v.is_empty())
        .unwrap_or(true)
    {
        eprintln!(
            "WAYLAND_DISPLAY unset; skipping probe_every_check_passes_on_a_healthy_host on a bare host"
        );
        return;
    }
    let host = FakeTerminalHost::new();
    let report = backend(&host).probe().await;
    assert!(report.available, "{report:?}");
    assert_eq!(report.backend, BackendId::Terminal);
    assert_eq!(report.checks.len(), 3, "{report:?}");
    assert!(report.checks.iter().all(|c| c.passed), "{report:?}");
    assert_eq!(report.version.as_deref(), Some("foot.desktop"));
}

#[tokio::test]
async fn probe_xdg_terminal_exec_missing_fails_the_first_check_only() {
    let host = FakeTerminalHost::new();
    let backend = XdgTerminalBackend::with_binaries(
        host.xdg_bin().with_file_name("no-such-binary"),
        host.hypr_bin(),
    );
    let report = backend.probe().await;
    assert!(!report.available);
    assert_eq!(report.checks.len(), 1, "{report:?}");
    assert!(!report.checks[0].passed);
    assert!(report.checks[0].remedy.is_some());
}

#[tokio::test]
async fn probe_xdg_terminal_exec_present_but_no_terminal_configured_fails_that_check() {
    let host = FakeTerminalHost::new();
    host.fail_print_id("no terminal entries found in xdg-terminals.list");
    let report = backend(&host).probe().await;
    assert!(!report.available, "{report:?}");
    assert_eq!(report.checks.len(), 1, "{report:?}");
    assert!(
        report.checks[0]
            .detail
            .contains("no terminal entries found"),
        "{report:?}"
    );
}

#[tokio::test]
async fn probe_hyprctl_unreachable_fails_that_check_only() {
    let host = FakeTerminalHost::new();
    host.fail_version("compositor not running");
    let report = backend(&host).probe().await;
    assert!(!report.available, "{report:?}");
    let hypr = report
        .checks
        .iter()
        .find(|c| c.name == "hyprctl answering")
        .expect("hyprctl check present");
    assert!(!hypr.passed);
    assert!(hypr.detail.contains("compositor not running"), "{hypr:?}");
}
