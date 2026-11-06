use std::time::{Duration, Instant};

use chrono::Utc;
use uuid::Uuid;

use super::cli::{DEFAULT_TIMEOUT, TmuxCli, argv};
use crate::launch::backend::{BackendId, Check, ValidationReport};

const MARKER: &str = "mike-tmux-validate-ok";
const MARKER_COMMAND: &str = "printf 'mike-tmux-%s\\n' validate-ok";
pub(super) const DEFAULT_CAPTURE_BUDGET: Duration = Duration::from_secs(3);
const CAPTURE_POLL: Duration = Duration::from_millis(150);

pub(super) async fn validate(cli: &TmuxCli, capture_budget: Duration) -> ValidationReport {
    let started_at = Utc::now();
    let mut checks = Vec::with_capacity(5);
    let scratch = match tempfile::Builder::new()
        .prefix("mike-tmux-validate-")
        .tempdir()
    {
        Ok(dir) => dir,
        Err(e) => {
            checks.push(Check::fail(
                "scratch directory",
                e.to_string(),
                "make the temp directory writable",
            ));
            return ValidationReport::from_checks(BackendId::Tmux, checks, started_at);
        }
    };
    let cwd = scratch.path().to_string_lossy().into_owned();
    let session = format!(
        "mike-validate-{}",
        &Uuid::new_v4().simple().to_string()[..8]
    );

    let created = new_session(cli, &session, &cwd).await;
    checks.push(created.clone());
    if !created.passed {
        return ValidationReport::from_checks(BackendId::Tmux, checks, started_at);
    }

    checks.push(send_marker(cli, &session).await);
    if checks.iter().all(|c| c.passed) {
        checks.push(capture_marker(cli, &session, capture_budget).await);
    }
    checks.push(list_panes(cli, &session).await);
    checks.push(kill_session(cli, &session).await);
    ValidationReport::from_checks(BackendId::Tmux, checks, started_at)
}

async fn new_session(cli: &TmuxCli, session: &str, cwd: &str) -> Check {
    let name = "new-session";
    let args = argv(&["new-session", "-d", "-s", session, "-c", cwd]);
    match cli.run_ok(&args, DEFAULT_TIMEOUT).await {
        Ok(()) => Check::pass(name, format!("created session {session}")),
        Err(e) => Check::fail(
            name,
            e.to_string(),
            "run `tmux new-session -d` by hand to see why",
        ),
    }
}

async fn send_marker(cli: &TmuxCli, session: &str) -> Check {
    let name = "send-keys";
    let args = argv(&["send-keys", "-t", session, MARKER_COMMAND, "Enter"]);
    match cli.run_ok(&args, DEFAULT_TIMEOUT).await {
        Ok(()) => Check::pass(name, format!("ran `{MARKER_COMMAND}` in {session}")),
        Err(e) => Check::fail(
            name,
            e.to_string(),
            "check the pane accepts keys (`tmux capture-pane`)",
        ),
    }
}

async fn capture_marker(cli: &TmuxCli, session: &str, capture_budget: Duration) -> Check {
    let name = "capture-pane";
    let deadline = Instant::now() + capture_budget;
    loop {
        match cli
            .run_text(
                &argv(&["capture-pane", "-p", "-t", session]),
                DEFAULT_TIMEOUT,
            )
            .await
        {
            Ok(text) if text.contains(MARKER) => {
                return Check::pass(name, format!("read back the `{MARKER}` marker"));
            }
            Ok(_) if Instant::now() < deadline => {
                tokio::time::sleep(CAPTURE_POLL).await;
            }
            Ok(text) => {
                return Check::fail(
                    name,
                    format!("no `{MARKER}` in the captured pane: {text:?}"),
                    "check the pane's shell prints command output",
                );
            }
            Err(e) => {
                return Check::fail(name, e.to_string(), "run `tmux capture-pane -p` by hand");
            }
        }
    }
}

async fn list_panes(cli: &TmuxCli, session: &str) -> Check {
    let name = "list-panes";
    match cli
        .run_text(
            &argv(&["list-panes", "-t", session, "-F", "#{pane_id}"]),
            DEFAULT_TIMEOUT,
        )
        .await
    {
        Ok(text) if !text.trim().is_empty() => {
            Check::pass(name, format!("{} pane(s)", text.lines().count()))
        }
        Ok(_) => Check::fail(name, "no panes listed", "run `tmux list-panes` by hand"),
        Err(e) => Check::fail(name, e.to_string(), "run `tmux list-panes` by hand"),
    }
}

async fn kill_session(cli: &TmuxCli, session: &str) -> Check {
    let name = "kill-session";
    match cli
        .run_ok(&argv(&["kill-session", "-t", session]), DEFAULT_TIMEOUT)
        .await
    {
        Ok(()) => Check::pass(name, format!("closed {session}")),
        Err(e) => Check::fail(
            name,
            e.to_string(),
            format!("close the scratch session by hand: `tmux kill-session -t {session}`"),
        ),
    }
}
