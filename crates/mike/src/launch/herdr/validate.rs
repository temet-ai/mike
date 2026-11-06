use std::time::Duration;

use chrono::Utc;
use serde_json::Value;

use super::cli::{DEFAULT_TIMEOUT, HerdrCli, argv, str_field};
use crate::launch::backend::{BackendId, Check, ValidationReport};

const MARKER: &str = "mike-validate-ok";
const MARKER_COMMAND: &str = "printf 'mike-%s\\n' validate-ok";
const OUTPUT_WAIT_MS: &str = "10000";
const OUTPUT_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) async fn validate(cli: &HerdrCli) -> ValidationReport {
    let started_at = Utc::now();
    let mut checks = Vec::with_capacity(5);
    let scratch = match tempfile::Builder::new().prefix("mike-validate-").tempdir() {
        Ok(dir) => dir,
        Err(e) => {
            checks.push(Check::fail(
                "scratch directory",
                e.to_string(),
                "make the temp directory writable",
            ));
            return ValidationReport::from_checks(BackendId::Herdr, checks, started_at);
        }
    };
    let cwd = scratch.path().to_string_lossy().into_owned();

    let created = create_workspace(cli, &cwd).await;
    let (workspace, pane) = match created {
        Ok((workspace, pane, check)) => {
            checks.push(check);
            (workspace, pane)
        }
        Err(check) => {
            checks.push(check);
            return ValidationReport::from_checks(BackendId::Herdr, checks, started_at);
        }
    };

    checks.push(run_marker(cli, &pane).await);
    if checks.iter().all(|c| c.passed) {
        checks.push(read_marker(cli, &pane).await);
    }
    checks.push(list_agents(cli).await);
    checks.push(close_workspace(cli, &workspace).await);
    ValidationReport::from_checks(BackendId::Herdr, checks, started_at)
}

async fn create_workspace(cli: &HerdrCli, cwd: &str) -> Result<(String, String, Check), Check> {
    let name = "workspace create";
    let args = argv(&[
        "workspace",
        "create",
        "--cwd",
        cwd,
        "--label",
        "mike-validate",
        "--no-focus",
    ]);
    let result = cli
        .run_json(&args, DEFAULT_TIMEOUT)
        .await
        .map_err(|e| Check::fail(name, e.to_string(), "start herdr so its server answers"))?;
    let workspace = result
        .pointer("/workspace/workspace_id")
        .and_then(Value::as_str);
    let pane = result.pointer("/root_pane/pane_id").and_then(Value::as_str);
    match (workspace, pane) {
        (Some(workspace), Some(pane)) => Ok((
            workspace.to_string(),
            pane.to_string(),
            Check::pass(name, format!("workspace {workspace}, root pane {pane}")),
        )),
        _ => Err(Check::fail(
            name,
            format!("no workspace_id/pane_id in {result}"),
            "update herdr; its `workspace create` answer changed shape",
        )),
    }
}

async fn run_marker(cli: &HerdrCli, pane: &str) -> Check {
    let name = "pane run";
    match cli
        .run_json(
            &argv(&["pane", "run", pane, MARKER_COMMAND]),
            DEFAULT_TIMEOUT,
        )
        .await
    {
        Ok(_) => Check::pass(name, format!("ran `{MARKER_COMMAND}` in {pane}")),
        Err(e) => Check::fail(
            name,
            e.to_string(),
            "check the pane accepts input (`herdr pane read`)",
        ),
    }
}

async fn read_marker(cli: &HerdrCli, pane: &str) -> Check {
    let name = "pane output";
    let args = argv(&[
        "pane",
        "wait-output",
        pane,
        "--match",
        MARKER,
        "--timeout",
        OUTPUT_WAIT_MS,
    ]);
    match cli.run_json(&args, OUTPUT_TIMEOUT).await {
        Ok(result) => {
            let line = str_field(&result, "matched_line").unwrap_or("");
            if line.contains(MARKER) {
                Check::pass(name, format!("read back `{}`", line.trim()))
            } else {
                Check::fail(
                    name,
                    format!("wait-output answered without the marker: {result}"),
                    "check the pane's shell prints command output (`herdr pane read`)",
                )
            }
        }
        Err(e) => Check::fail(
            name,
            e.to_string(),
            "check the pane's shell prints command output (`herdr pane read`)",
        ),
    }
}

async fn list_agents(cli: &HerdrCli) -> Check {
    let name = "agent list";
    match cli
        .run_json(&argv(&["agent", "list"]), DEFAULT_TIMEOUT)
        .await
    {
        Ok(result) => match result.get("agents").and_then(Value::as_array) {
            Some(agents) => Check::pass(name, format!("{} live agent(s)", agents.len())),
            None => Check::fail(name, format!("no agents array in {result}"), "update herdr"),
        },
        Err(e) => Check::fail(name, e.to_string(), "start herdr so its server answers"),
    }
}

async fn close_workspace(cli: &HerdrCli, workspace: &str) -> Check {
    let name = "workspace close";
    match cli
        .run_json(&argv(&["workspace", "close", workspace]), DEFAULT_TIMEOUT)
        .await
    {
        Ok(_) => Check::pass(name, format!("closed {workspace}")),
        Err(e) => Check::fail(
            name,
            e.to_string(),
            format!("close the scratch workspace by hand: `herdr workspace close {workspace}`"),
        ),
    }
}
