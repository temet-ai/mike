use std::time::Duration;

use serde_json::Value;

use super::HERDR_PROTOCOL;
use super::cli::{HerdrCli, INSTALL_REMEDY, argv};
use crate::launch::backend::{BackendId, Check, ProbeReport};

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn probe(cli: &HerdrCli) -> ProbeReport {
    let mut checks = Vec::with_capacity(4);
    let mut report = ProbeReport {
        backend: BackendId::Herdr,
        available: false,
        version: None,
        protocol: None,
        checks: Vec::new(),
    };

    match cli.run_text(&argv(&["--version"]), PROBE_TIMEOUT).await {
        Ok(text) => {
            let version = text.trim().trim_start_matches("herdr").trim().to_string();
            checks.push(Check::pass(
                "herdr on PATH",
                format!("herdr {version} at {}", cli.bin().display()),
            ));
            report.version = Some(version);
        }
        Err(err) => {
            checks.push(Check::fail(
                "herdr on PATH",
                err.to_string(),
                err.remedy().unwrap_or(INSTALL_REMEDY),
            ));
            report.checks = checks;
            return report;
        }
    }

    let protocol = match cli
        .run_json(&argv(&["api", "snapshot"]), PROBE_TIMEOUT)
        .await
    {
        Ok(result) => {
            let protocol = result.pointer("/snapshot/protocol").and_then(Value::as_u64);
            checks.push(Check::pass("herdr server answers", "api snapshot answered"));
            protocol
        }
        Err(err) => {
            checks.push(Check::fail(
                "herdr server answers",
                err.to_string(),
                "start herdr (run `herdr` in a terminal) so its server socket answers",
            ));
            None
        }
    };
    if let Some(protocol) = protocol {
        report.protocol = Some(protocol.to_string());
        checks.push(protocol_check(protocol));
    }

    checks.push(integration_check(cli).await);
    report.available = checks.iter().all(|c| c.passed);
    report.checks = checks;
    report
}

fn protocol_check(protocol: u64) -> Check {
    if protocol == u64::from(HERDR_PROTOCOL) {
        Check::pass("herdr protocol", format!("protocol {protocol}"))
    } else {
        Check::fail(
            "herdr protocol",
            format!("server speaks protocol {protocol}; this mike expects {HERDR_PROTOCOL}"),
            "update herdr (`herdr update`) or mike so the protocol versions match",
        )
    }
}

async fn integration_check(cli: &HerdrCli) -> Check {
    let name = "claude integration";
    let remedy = "run `herdr integration install claude`";
    match cli
        .run_text(&argv(&["integration", "status"]), PROBE_TIMEOUT)
        .await
    {
        Ok(text) => match claude_integration_line(&text) {
            Some(line) if line.contains("current") => Check::pass(name, line),
            Some(line) => Check::fail(name, line, remedy),
            None => Check::fail(
                name,
                "no `claude:` line in `herdr integration status`",
                remedy,
            ),
        },
        Err(err) => Check::fail(name, err.to_string(), remedy),
    }
}

fn claude_integration_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| l.starts_with("claude:"))
        .map(str::to_string)
}
