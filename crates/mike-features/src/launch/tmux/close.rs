use std::time::Duration;

use super::cli::{DEFAULT_TIMEOUT, TmuxCli, argv};
use super::surface::{pane_exists, wait_for_exit};
use crate::launch::backend::{CloseOutcome, Surface};
use crate::launch::error::LaunchError;

pub async fn close(
    cli: &TmuxCli,
    surface: &Surface,
    wait: Duration,
) -> Result<CloseOutcome, LaunchError> {
    let pane = &surface.surface_ref;
    if !pane_exists(cli, pane).await? {
        return Ok(CloseOutcome::NotFound);
    }
    let closed = cli
        .run_missing_ok(&argv(&["kill-pane", "-t", pane]), DEFAULT_TIMEOUT)
        .await?;
    if !closed {
        return Ok(CloseOutcome::NotFound);
    }
    let process_exited = match surface.pid {
        Some(pid) => wait_for_exit(pid, wait).await,
        None => false,
    };
    Ok(CloseOutcome::Closed { process_exited })
}
