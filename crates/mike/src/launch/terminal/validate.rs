use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use chrono::Utc;

use super::TerminalTimeouts;
use super::cli::{DEFAULT_TIMEOUT, TerminalCli};
use super::surface;
use crate::launch::backend::{BackendId, Check, ValidationReport};

const MARKER: &str = "mike-terminal-validate-ok";
const OPEN_POLL: std::time::Duration = std::time::Duration::from_millis(150);

pub(super) async fn validate(cli: &TerminalCli, timeouts: TerminalTimeouts) -> ValidationReport {
    let started_at = Utc::now();
    let mut checks = Vec::with_capacity(5);
    let scratch = match tempfile::Builder::new()
        .prefix("mike-terminal-validate-")
        .tempdir()
    {
        Ok(dir) => dir,
        Err(e) => {
            checks.push(Check::fail(
                "scratch directory",
                e.to_string(),
                "make the temp directory writable",
            ));
            return ValidationReport::from_checks(BackendId::Terminal, checks, started_at);
        }
    };
    let cwd = scratch.path().to_string_lossy().into_owned();
    let marker = scratch.path().join("marker");

    let window_pid = match open_marker_window(cli, &cwd, &marker, timeouts).await {
        Ok((pid, check)) => {
            checks.push(check);
            Some(pid)
        }
        Err(check) => {
            checks.push(check);
            None
        }
    };
    if let Some(pid) = window_pid {
        checks.push(read_marker(&marker, timeouts).await);
        checks.push(self_close(pid, timeouts).await);
    }
    checks.push(forced_close(cli, &cwd, timeouts).await);

    ValidationReport::from_checks(BackendId::Terminal, checks, started_at)
}

async fn open_marker_window(
    cli: &TerminalCli,
    cwd: &str,
    marker: &Path,
    timeouts: TerminalTimeouts,
) -> Result<(u32, Check), Check> {
    let name = "window opens";
    let script = format!(
        "printf '%s\\n' {MARKER} > {}; sleep 1; exit 0",
        shell_quote(&marker.to_string_lossy())
    );
    let command = vec!["sh".to_string(), "-c".to_string(), script];
    let child = cli
        .spawn_xdg(
            cwd,
            "mike-validate",
            "mike-validate",
            &command,
            &BTreeMap::new(),
        )
        .map_err(|e| {
            Check::fail(
                name,
                e.to_string(),
                "check `xdg-terminal-exec` runs by hand",
            )
        })?;
    let Some(pid) = child.id() else {
        return Err(Check::fail(
            name,
            "xdg-terminal-exec exited before reporting a pid",
            "run it by hand to see why",
        ));
    };
    let found = wait_for_pid_in_clients(cli, pid, timeouts.window).await;
    surface::spawn_reaper(child);
    if found {
        Ok((
            pid,
            Check::pass(
                name,
                format!("window pid {pid} appeared in `hyprctl clients`"),
            ),
        ))
    } else {
        Err(Check::fail(
            name,
            format!(
                "window pid {pid} never appeared in `hyprctl clients` within {:?}",
                timeouts.window
            ),
            "check `hyprctl clients -j` by hand while a window is open",
        ))
    }
}

async fn wait_for_pid_in_clients(cli: &TerminalCli, pid: u32, budget: std::time::Duration) -> bool {
    let deadline = Instant::now() + budget;
    loop {
        if matches!(
            surface::find_by_pid(cli, pid, DEFAULT_TIMEOUT).await,
            Ok(Some(_))
        ) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(OPEN_POLL).await;
    }
}

async fn read_marker(path: &Path, timeouts: TerminalTimeouts) -> Check {
    let name = "command ran";
    let deadline = Instant::now() + timeouts.marker;
    loop {
        if let Ok(text) = std::fs::read_to_string(path)
            && text.trim() == MARKER
        {
            return Check::pass(name, format!("read back `{}`", text.trim()));
        }
        if Instant::now() >= deadline {
            return Check::fail(
                name,
                "the marker file was never written",
                "check the terminal's shell actually runs its command (`xdg-terminal-exec -- sh -c '...'` by hand)",
            );
        }
        tokio::time::sleep(OPEN_POLL).await;
    }
}

async fn self_close(pid: u32, timeouts: TerminalTimeouts) -> Check {
    let name = "window closes on its own";
    if surface::wait_for_exit(pid, timeouts.self_close).await {
        Check::pass(
            name,
            format!("window pid {pid} exited without being closed"),
        )
    } else {
        Check::fail(
            name,
            format!(
                "window pid {pid} was still running after {:?}",
                timeouts.self_close
            ),
            "the terminal may be configured to hold after the command exits (`--hold`); check its config",
        )
    }
}

async fn forced_close(cli: &TerminalCli, cwd: &str, timeouts: TerminalTimeouts) -> Check {
    let name = "forced close";
    let command = vec!["sh".to_string(), "-c".to_string(), "sleep 30".to_string()];
    let child = match cli.spawn_xdg(
        cwd,
        "mike-validate-close",
        "mike-validate-close",
        &command,
        &BTreeMap::new(),
    ) {
        Ok(c) => c,
        Err(e) => {
            return Check::fail(
                name,
                e.to_string(),
                "check `xdg-terminal-exec` runs by hand",
            );
        }
    };
    let Some(pid) = child.id() else {
        surface::spawn_reaper(child);
        return Check::fail(
            name,
            "xdg-terminal-exec exited before reporting a pid",
            "run it by hand to see why",
        );
    };
    let found = wait_for_pid_in_clients(cli, pid, timeouts.window).await;
    surface::spawn_reaper(child);
    if !found {
        return Check::fail(
            name,
            format!("window pid {pid} never appeared to close-test"),
            "check `hyprctl clients -j` by hand while a window is open",
        );
    }
    if let Err(e) = surface::terminate(pid) {
        return Check::fail(
            name,
            e.to_string(),
            format!("send it a SIGTERM by hand: `kill {pid}`"),
        );
    }
    if surface::wait_for_exit(pid, timeouts.forced_close).await {
        Check::pass(name, format!("closed window pid {pid}"))
    } else {
        Check::fail(
            name,
            format!(
                "window pid {pid} was still running {:?} after SIGTERM",
                timeouts.forced_close
            ),
            format!("send it a SIGTERM by hand: `kill {pid}`"),
        )
    }
}

fn shell_quote(arg: &str) -> String {
    let safe = arg
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_./:=,@".contains(c));
    if safe && !arg.is_empty() {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_bare_tokens_pass_through_and_others_are_single_quoted() {
        assert_eq!(shell_quote("/tmp/a/marker"), "/tmp/a/marker");
        assert_eq!(shell_quote("has space"), "'has space'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }
}
