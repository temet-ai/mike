#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use serde_json::json;

use super::{FakeHerdr, envelope};
use crate::launch::backend::TerminalBackend;
use crate::launch::error::LaunchError;
use crate::launch::herdr::HerdrBackend;
use crate::launch::herdr::cli::{HerdrCli, argv};

const INTEGRATION_OK: &str = "pi: current (v8) (/x)\nclaude: current (v9) (/home/u/.claude/hooks/herdr-agent-state.sh)\ncodex: current (v8) (/y)\n";

fn snapshot(protocol: u64) -> String {
    envelope(
        json!({"snapshot": {"agents": [], "protocol": protocol, "version": "0.9.0"}, "type": "session_snapshot"}),
    )
}

#[tokio::test]
async fn probe_found_server_protocol_and_integration_is_available() {
    let fake = FakeHerdr::new();
    fake.answer("--version", "herdr 0.9.0\n")
        .answer("api snapshot", &snapshot(22))
        .answer("integration status", INTEGRATION_OK);
    let report = HerdrBackend::with_binary(fake.bin()).probe().await;
    assert!(report.available, "{report:?}");
    assert_eq!(report.version.as_deref(), Some("0.9.0"));
    assert_eq!(report.protocol.as_deref(), Some("22"));
    let names: Vec<&str> = report.checks.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "herdr on PATH",
            "herdr server answers",
            "herdr protocol",
            "claude integration"
        ]
    );
    assert!(report.checks.iter().all(|c| c.passed));
}

#[tokio::test]
async fn probe_protocol_mismatch_is_a_failed_check() {
    let fake = FakeHerdr::new();
    fake.answer("--version", "herdr 0.9.0")
        .answer("api snapshot", &snapshot(23))
        .answer("integration status", INTEGRATION_OK);
    let report = HerdrBackend::with_binary(fake.bin()).probe().await;
    assert!(!report.available);
    let protocol = report
        .checks
        .iter()
        .find(|c| c.name == "herdr protocol")
        .unwrap();
    assert!(!protocol.passed);
    assert!(protocol.detail.contains("23") && protocol.detail.contains("22"));
    assert!(protocol.remedy.as_deref().unwrap().contains("herdr update"));
}

#[tokio::test]
async fn probe_missing_binary_fails_first_check_with_install_remedy() {
    let report = HerdrBackend::with_binary("/nonexistent/dir/herdr")
        .probe()
        .await;
    assert!(!report.available);
    assert_eq!(report.checks.len(), 1);
    assert!(!report.checks[0].passed);
    assert!(
        report.checks[0]
            .remedy
            .as_deref()
            .unwrap()
            .contains("https://herdr.dev")
    );
}

#[tokio::test]
async fn probe_server_down_and_integration_missing_name_remedies() {
    let fake = FakeHerdr::new();
    fake.answer("--version", "herdr 0.9.0")
        .error("api snapshot", "connect_failed", "no server")
        .answer("integration status", "claude: not installed (/x)\n");
    let report = HerdrBackend::with_binary(fake.bin()).probe().await;
    assert!(!report.available);
    assert_eq!(report.protocol, None);
    let server = report
        .checks
        .iter()
        .find(|c| c.name == "herdr server answers")
        .unwrap();
    assert!(!server.passed && server.remedy.as_deref().unwrap().contains("start herdr"));
    let integration = report
        .checks
        .iter()
        .find(|c| c.name == "claude integration")
        .unwrap();
    assert!(!integration.passed);
    assert_eq!(
        integration.remedy.as_deref(),
        Some("run `herdr integration install claude`")
    );
    assert!(report.checks.iter().all(|c| c.name != "herdr protocol"));
}

#[tokio::test]
async fn cli_run_json_hung_call_is_timeout_naming_the_command() {
    let fake = FakeHerdr::new();
    fake.hang("api snapshot");
    let cli = HerdrCli::new(fake.bin());
    let err = cli
        .run_json(
            &argv(&["api", "snapshot"]),
            std::time::Duration::from_millis(300),
        )
        .await
        .unwrap_err();
    match err {
        LaunchError::Timeout { command, .. } => assert_eq!(command, "herdr api snapshot"),
        other => panic!("expected Timeout, got {other:?}"),
    }
}

#[tokio::test]
async fn cli_run_json_non_json_stdout_and_missing_result_are_malformed() {
    let fake = FakeHerdr::new();
    fake.answer("agent list", "not json")
        .answer("tab list", r#"{"id":"x"}"#);
    let cli = HerdrCli::new(fake.bin());
    let err = cli
        .run_json(&argv(&["agent", "list"]), std::time::Duration::from_secs(5))
        .await
        .unwrap_err();
    assert!(matches!(err, LaunchError::Malformed { .. }), "{err}");
    let err = cli
        .run_json(&argv(&["tab", "list"]), std::time::Duration::from_secs(5))
        .await
        .unwrap_err();
    assert!(matches!(err, LaunchError::Malformed { .. }), "{err}");
}

#[tokio::test]
async fn cli_non_json_stderr_failure_is_failed_with_status() {
    let fake = FakeHerdr::new();
    let dir = fake.bin();
    std::fs::write(
        dir.parent().unwrap().join("plain"),
        "#!/bin/sh\necho boom >&2\nexit 3\n",
    )
    .unwrap();
    std::fs::set_permissions(
        dir.parent().unwrap().join("plain"),
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    let cli = HerdrCli::new(dir.parent().unwrap().join("plain"));
    let err = cli
        .run_text(&argv(&["--version"]), std::time::Duration::from_secs(5))
        .await
        .unwrap_err();
    match err {
        LaunchError::Failed(msg) => assert!(msg.contains("boom") && msg.contains("3"), "{msg}"),
        other => panic!("expected Failed, got {other:?}"),
    }
}
