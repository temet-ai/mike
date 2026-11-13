use std::time::Duration;

use super::cli::{DEFAULT_TIMEOUT, TerminalCli};
use super::surface::{self, app_id_for_session, wait_for_exit};
use crate::launch::backend::{CloseOutcome, Surface};
use crate::launch::error::LaunchError;

pub async fn close(
    cli: &TerminalCli,
    surface_val: &Surface,
    wait: Duration,
) -> Result<CloseOutcome, LaunchError> {
    let Ok(window_pid) = surface_val.surface_ref.parse::<u32>() else {
        return Ok(CloseOutcome::NotFound);
    };
    let Some(entry) = surface::find_by_pid(cli, window_pid, DEFAULT_TIMEOUT).await? else {
        return Ok(CloseOutcome::NotFound);
    };
    if let Some(expected) = surface_val.session_uuid {
        let expected_tag = app_id_for_session(expected);
        if entry.class != expected_tag {
            return Ok(CloseOutcome::Mismatch {
                detail: format!(
                    "window {window_pid} carries tag {}, not {expected_tag}",
                    entry.class
                ),
            });
        }
    }
    surface::terminate(window_pid)?;
    let watch_pid = surface_val.pid.unwrap_or(window_pid);
    let process_exited = wait_for_exit(watch_pid, wait).await;
    Ok(CloseOutcome::Closed { process_exited })
}
