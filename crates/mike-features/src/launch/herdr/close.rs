use std::time::{Duration, Instant};

use super::cli::{DEFAULT_TIMEOUT, HerdrCli, argv, str_field};
use super::surface::{agent_session_uuid, claude_agents, pane_exists};
use crate::launch::backend::{CloseOutcome, Surface};
use crate::launch::error::LaunchError;

const PID_POLL: Duration = Duration::from_millis(200);

pub async fn close(
    cli: &HerdrCli,
    surface: &Surface,
    wait: Duration,
) -> Result<CloseOutcome, LaunchError> {
    let pane = &surface.surface_ref;
    if !pane_exists(cli, pane).await? {
        return Ok(CloseOutcome::NotFound);
    }
    let mismatch = match surface.session_uuid {
        Some(expected) => session_mismatch(cli, pane, expected).await?,
        None => None,
    };
    if let Some(detail) = mismatch {
        return Ok(CloseOutcome::Mismatch { detail });
    }
    match cli
        .run_json(&argv(&["pane", "close", pane]), DEFAULT_TIMEOUT)
        .await
    {
        Ok(_) => {}
        Err(err) if err.backend_code() == Some("pane_not_found") => {
            return Ok(CloseOutcome::NotFound);
        }
        Err(err) => return Err(err),
    }
    let process_exited = match surface.pid {
        Some(pid) => wait_for_exit(pid, wait).await,
        None => false,
    };
    Ok(CloseOutcome::Closed { process_exited })
}

async fn session_mismatch(
    cli: &HerdrCli,
    pane: &str,
    expected: uuid::Uuid,
) -> Result<Option<String>, LaunchError> {
    let agents = claude_agents(cli).await?;
    let Some(agent) = agents
        .iter()
        .find(|a| str_field(a, "pane_id") == Some(pane))
    else {
        return Ok(Some(format!(
            "pane {pane} hosts no claude session (expected {expected})"
        )));
    };
    match agent_session_uuid(agent) {
        Some(actual) if actual == expected => Ok(None),
        Some(actual) => Ok(Some(format!(
            "pane {pane} hosts session {actual}, not {expected}"
        ))),
        None => Ok(Some(format!(
            "pane {pane} hosts a claude agent with no known session id (expected {expected})"
        ))),
    }
}

async fn wait_for_exit(pid: u32, wait: Duration) -> bool {
    let deadline = Instant::now() + wait;
    loop {
        if i32::try_from(pid)
            .ok()
            .and_then(crate::launch::local_root::process_present)
            == Some(false)
        {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(PID_POLL.min(deadline.saturating_duration_since(Instant::now()))).await;
    }
}

pub async fn close_workspace(cli: &HerdrCli, workspace: &str) -> Result<(), LaunchError> {
    match cli
        .run_json(&argv(&["workspace", "close", workspace]), DEFAULT_TIMEOUT)
        .await
    {
        Ok(_) => Ok(()),
        Err(err) if err.backend_code() == Some("workspace_not_found") => Ok(()),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn wait_for_exit_gone_pid_is_true_live_pid_is_false() {
        assert!(wait_for_exit(i32::MAX as u32, Duration::from_millis(10)).await);
        assert!(!wait_for_exit(std::process::id(), Duration::from_millis(50)).await);
    }
}
