use std::time::Duration;

use chrono::Utc;

use super::cli::{HcCli, INSTALL_REMEDY, argv};
use crate::launch::backend::{BackendId, Check, ProbeReport, ValidationReport};

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) async fn probe(cli: &HcCli) -> ProbeReport {
    let mut report = ProbeReport {
        backend: BackendId::HerdrCore,
        available: false,
        version: None,
        protocol: None,
        checks: Vec::new(),
    };
    match cli.hc_text(&argv(&["--version"]), PROBE_TIMEOUT).await {
        Ok(text) => {
            let version = text.trim().trim_start_matches("hc").trim().to_string();
            report.checks.push(Check::pass(
                "hc on PATH",
                format!("hc {version} at {}", cli.hc_bin().display()),
            ));
            report.version = Some(version);
        }
        Err(err) => {
            report.checks.push(Check::fail(
                "hc on PATH",
                err.to_string(),
                err.remedy().unwrap_or(INSTALL_REMEDY),
            ));
            return report;
        }
    }
    report.checks.push(server_check(cli).await);
    report.checks.push(app_check(cli).await);
    report.available = report.checks.iter().all(|c| c.passed);
    report
}

pub(super) async fn validate(cli: &HcCli) -> ValidationReport {
    let started_at = Utc::now();
    let report = probe(cli).await;
    ValidationReport::from_checks(BackendId::HerdrCore, report.checks, started_at)
}

async fn server_check(cli: &HcCli) -> Check {
    match cli.hc_json(&argv(&["ls", "--json"]), PROBE_TIMEOUT).await {
        Ok(_) => Check::pass("herdr-core answers", "hc ls answered"),
        Err(err) => Check::fail(
            "herdr-core answers",
            err.to_string(),
            "start the Herdr app, or `hc server start`, so herdr-core answers",
        ),
    }
}

async fn app_check(cli: &HcCli) -> Check {
    match cli.app_json(&argv(&["ls"]), PROBE_TIMEOUT).await {
        Ok(_) => Check::pass("Herdr app window", "herdr-app ls answered"),
        Err(err) => Check::fail(
            "Herdr app window",
            err.to_string(),
            "open the Herdr app (`herdr-app`); sessions started meanwhile appear when it opens",
        ),
    }
}
