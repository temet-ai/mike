#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]

use crate::launch::backend::TerminalBackend;
use crate::launch::herdr::HerdrBackend;

#[tokio::test]
async fn live_validate_passes_against_the_real_herdr_when_opted_in() {
    if std::env::var("MIKE_LIVE_HERDR").as_deref() != Ok("1") {
        return;
    }
    let backend = HerdrBackend::new();
    let probe = backend.probe().await;
    for check in &probe.checks {
        println!(
            "probe  {} {} — {}",
            if check.passed { "PASS" } else { "FAIL" },
            check.name,
            check.detail
        );
    }
    let report = backend.validate().await;
    for check in &report.checks {
        println!(
            "validate {} {} — {}{}",
            if check.passed { "PASS" } else { "FAIL" },
            check.name,
            check.detail,
            check
                .remedy
                .as_ref()
                .map(|r| format!(" (remedy: {r})"))
                .unwrap_or_default()
        );
    }
    assert!(probe.available, "{probe:?}");
    assert!(report.passed, "{report:?}");
}
