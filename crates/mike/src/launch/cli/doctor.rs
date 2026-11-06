use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Serialize;

use super::ledger::Ledger;
use super::render::CHOOSE_BACKEND;
use super::runner::Runner;
use super::sidecar;
use crate::launch::{BackendId, Check, ProbeReport, ValidationReport};
use crate::store::persistence::launch::NewBackendValidation;

pub(crate) struct DoctorEnv {
    pub(crate) sidecar_url: String,
    pub(crate) inspect_addr: String,
    pub(crate) ca_candidates: Vec<PathBuf>,
    pub(crate) claude_bin: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DoctorReport {
    pub(crate) backend: BackendId,
    pub(crate) passed: bool,
    pub(crate) checks: Vec<Check>,
}

impl DoctorReport {
    pub(crate) fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| {
            format!(
                "{{\"backend\":\"{}\",\"passed\":false,\"checks\":[],\"error\":\"json encoding failed: {e}\"}}",
                self.backend
            )
        })
    }
}

pub(crate) async fn execute(runner: &Runner<'_>, env: &DoctorEnv) -> DoctorReport {
    let id = runner.backend.id();
    let mut checks = vec![backend_chosen(id)];
    let probe = runner.backend.probe().await;
    checks.extend(prefixed("probe", &probe.checks));
    let validation = if probe.available {
        if id != BackendId::None {
            eprintln!(
                "mike spawn doctor: validating {id} live — this opens and closes a scratch workspace"
            );
        }
        Some(runner.backend.validate().await)
    } else {
        None
    };
    match &validation {
        Some(report) => checks.extend(prefixed("validate", &report.checks)),
        None => checks.push(Check::fail(
            "validate",
            "skipped: the probe failed",
            "fix the probe checks above, then run doctor again",
        )),
    }
    checks.push(sidecar_check(&env.sidecar_url).await);
    checks.push(ca_cert_check(&env.ca_candidates));
    checks.push(claude_check(&env.claude_bin));
    checks.push(inspect_api_check(&env.inspect_addr).await);
    checks.push(database_check(&runner.ledger));
    checks.push(mcp_server_registered_check(runner.trust.config_path()));
    checks.push(previous_validation_check(&runner.ledger, id).await);
    if let Some(report) = &validation {
        record(&runner.ledger, &probe, report).await;
    }
    DoctorReport {
        backend: id,
        passed: checks.iter().all(|c| c.passed),
        checks,
    }
}

fn prefixed(prefix: &str, checks: &[Check]) -> Vec<Check> {
    checks
        .iter()
        .map(|c| Check {
            name: format!("{prefix}: {}", c.name),
            ..c.clone()
        })
        .collect()
}

fn backend_chosen(id: BackendId) -> Check {
    if id == BackendId::None {
        Check::fail("backend chosen", "launch_backend is `none`", CHOOSE_BACKEND)
    } else {
        Check::pass("backend chosen", id.as_str())
    }
}

async fn sidecar_check(url: &str) -> Check {
    match sidecar::sidecar_reachable(url).await {
        Ok(status) => Check::pass("sidecar", format!("{url} answered HTTP {status}")),
        Err(why) => Check::fail(
            "sidecar",
            why,
            format!(
                "start proxy-forward on {url} (scripts/start-proxy.sh brings it up) or set \
                 MIKE_LAUNCH_PROXY_FORWARD_URL"
            ),
        ),
    }
}

fn ca_cert_check(candidates: &[PathBuf]) -> Check {
    let readable = candidates
        .iter()
        .find(|p| std::fs::read(p).is_ok_and(|bytes| !bytes.is_empty()));
    match readable {
        Some(path) => Check::pass("ca cert", path.to_string_lossy()),
        None => Check::fail(
            "ca cert",
            format!(
                "no readable CA certificate at {}",
                candidates
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "start proxy-forward once so it writes its CA, or set MIKE_LAUNCH_CA_CERT",
        ),
    }
}

fn claude_check(bin: &str) -> Check {
    match sidecar::locate_executable(bin, std::env::var_os("PATH").as_deref()) {
        Some(path) => Check::pass("claude", path.to_string_lossy()),
        None => Check::fail(
            "claude",
            format!("`{bin}` is not an executable on PATH"),
            "install Claude Code so `claude` is on PATH, or set MIKE_CLAUDE_BIN",
        ),
    }
}

async fn inspect_api_check(inspect_addr: &str) -> Check {
    if sidecar::inspect_api_reachable(inspect_addr).await {
        Check::pass("inspect api", format!("http://{inspect_addr}/api/health"))
    } else {
        Check::fail(
            "inspect api",
            format!("http://{inspect_addr}/api/health did not answer"),
            "start the proxy (`mike serve`, or `mike claude` brings it up)",
        )
    }
}

fn database_check(ledger: &Ledger) -> Check {
    if ledger.available() {
        Check::pass(
            "database",
            format!("launches are recorded through {}", ledger.route()),
        )
    } else {
        Check::fail(
            "database",
            "unreachable or persistence disabled; launches will not be recorded and the \
             channel check reports db_unreachable",
            "check MIKE_DATABASE_URL and that Postgres is up",
        )
    }
}

fn mcp_server_registered_check(path: &Path) -> Check {
    let contents = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return Check::fail(
                "mcp server",
                format!("cannot read {}: {e}", path.display()),
                "run Claude Code once so it creates ~/.claude.json, or check permissions",
            );
        }
    };
    let parsed: serde_json::Value = match serde_json::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            return Check::fail(
                "mcp server",
                format!("{} is not valid JSON: {e}", path.display()),
                "fix or regenerate ~/.claude.json",
            );
        }
    };
    if parsed
        .get("mcpServers")
        .and_then(|servers| servers.get("mike"))
        .is_some()
    {
        Check::pass(
            "mcp server",
            format!("mike registered in {}", path.display()),
        )
    } else {
        Check::fail(
            "mcp server",
            format!("no mcpServers.mike entry in {}", path.display()),
            "run `claude mcp add --scope user mike -- mike-mcp` so \
             `--dangerously-load-development-channels server:mike` resolves at launch; a \
             server supplied only via --mcp-config does not count",
        )
    }
}

async fn previous_validation_check(ledger: &Ledger, id: BackendId) -> Check {
    match ledger.latest_validation(id.as_str()).await {
        Ok(Some(row)) => {
            let age = Utc::now().signed_duration_since(row.finished_at);
            Check::pass(
                "last validation",
                format!(
                    "{}s ago by {} ({})",
                    age.num_seconds().max(0),
                    row.source,
                    if row.passed { "passed" } else { "failed" }
                ),
            )
        }
        Ok(None) => Check::pass(
            "last validation",
            "none on record before this run; this doctor records one",
        ),
        Err(why) => Check::fail(
            "last validation",
            format!("cannot read launch.backend_validations: {why}"),
            "check MIKE_DATABASE_URL and that Postgres is up",
        ),
    }
}

async fn record(ledger: &Ledger, probe: &ProbeReport, report: &ValidationReport) {
    if report.backend == BackendId::None || !ledger.available() {
        return;
    }
    let row = NewBackendValidation {
        backend: report.backend.as_str().to_string(),
        passed: report.passed,
        version: probe.version.clone(),
        protocol: probe.protocol.clone(),
        checks: serde_json::to_value(&report.checks).unwrap_or(serde_json::Value::Null),
        started_at: report.started_at,
        finished_at: report.finished_at,
        source: "doctor".to_string(),
    };
    if let Err(e) = ledger.insert_validation(&row).await {
        eprintln!("mike spawn doctor: could not record the validation: {e}");
    }
}
