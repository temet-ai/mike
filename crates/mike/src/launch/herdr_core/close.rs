use std::time::{Duration, Instant};

use super::cli::{DEFAULT_TIMEOUT, HcCli, argv};
use super::terminal::{self, SPACE_LABEL, Terminal};
use crate::launch::backend::{CloseOutcome, Surface};
use crate::launch::error::LaunchError;

const PID_POLL: Duration = Duration::from_millis(200);

pub(super) fn session_mismatch(terminal: &Terminal, expected: uuid::Uuid) -> Option<String> {
    let address = &terminal.address;
    match terminal.session {
        Some(actual) if actual == expected => None,
        Some(actual) => Some(format!(
            "terminal {address} hosts session {actual}, not {expected}"
        )),
        None => Some(format!(
            "terminal {address} carries no mike session label (expected {expected})"
        )),
    }
}

pub(super) async fn close(
    cli: &HcCli,
    surface: &Surface,
    wait: Duration,
) -> Result<CloseOutcome, LaunchError> {
    let Some(current) = terminal::get(cli, &surface.surface_ref).await? else {
        return Ok(CloseOutcome::NotFound);
    };
    if !current.live {
        return Ok(CloseOutcome::NotFound);
    }
    if let Some(detail) = surface
        .session_uuid
        .and_then(|expected| session_mismatch(&current, expected))
    {
        return Ok(CloseOutcome::Mismatch { detail });
    }
    match cli
        .hc_json(
            &argv(&["kill", "--json", &current.address]),
            DEFAULT_TIMEOUT,
        )
        .await
    {
        Ok(_) => {}
        Err(err) if err.backend_code() == Some("not_found") => return Ok(CloseOutcome::NotFound),
        Err(err) => return Err(err),
    }
    let pid = surface.pid.or(current.pid);
    let process_exited = match pid {
        Some(pid) => wait_for_exit(pid, wait).await,
        None => false,
    };
    Ok(CloseOutcome::Closed { process_exited })
}

pub(super) async fn close_workspace(cli: &HcCli, space: &str) -> Result<(), LaunchError> {
    for address in terminal::addresses_labelled(cli, SPACE_LABEL, space).await? {
        match cli
            .hc_json(&argv(&["kill", "--json", &address]), DEFAULT_TIMEOUT)
            .await
        {
            Ok(_) => {}
            Err(err) if err.backend_code() == Some("not_found") => {}
            Err(err) => return Err(err),
        }
    }
    Ok(())
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
