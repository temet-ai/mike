use std::collections::BTreeMap;
use std::time::Duration;

use chrono::Utc;
use serde_json::Value;

use super::cli::{HcCli, INSTALL_REMEDY, argv};
use super::open::{agent_command, with_caller_path};
use crate::launch::backend::{BackendId, Check, ProbeReport, ValidationReport};

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const AGENT_TIMEOUT: Duration = Duration::from_secs(30);
const AGENT: &str = "claude";
const AGENT_CHECK: &str = "claude starts in herdr-core";
const AGENT_REMEDY: &str = "put claude on mike's PATH, or restart herdr-core from a shell that has it (`hc server restart`)";

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
    let mut report = probe(cli).await;
    if report.available {
        report.checks.push(agent_check(cli).await);
    }
    ValidationReport::from_checks(BackendId::HerdrCore, report.checks, started_at)
}

async fn agent_check(cli: &HcCli) -> Check {
    let env = with_caller_path(&BTreeMap::new(), std::env::var("PATH").ok());
    let command = match agent_command(AGENT, &["--version".to_string()], &env) {
        Ok(command) => command,
        Err(err) => return Check::fail(AGENT_CHECK, err.to_string(), AGENT_REMEDY),
    };
    let mut args = argv(&["run", "--json", "--rm", "--no-parent", "--no-notify"]);
    args.extend(command);
    match cli.hc_json(&args, AGENT_TIMEOUT).await {
        Ok(answer) => agent_ran(&answer),
        Err(err) => Check::fail(AGENT_CHECK, err.to_string(), AGENT_REMEDY),
    }
}

pub(super) fn agent_ran(answer: &Value) -> Check {
    let output = answer["output"].as_str().unwrap_or_default().trim();
    match answer["wait"]["exit_code"].as_i64() {
        Some(0) => Check::pass(
            AGENT_CHECK,
            format!("herdr-core ran `{AGENT} --version`: {output}"),
        ),
        code => Check::fail(
            AGENT_CHECK,
            format!("herdr-core ran `{AGENT} --version`, exit {code:?}: {output}"),
            AGENT_REMEDY,
        ),
    }
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
