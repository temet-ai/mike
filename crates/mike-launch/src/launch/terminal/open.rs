use std::time::Instant;

use super::TerminalTimeouts;
use super::cli::TerminalCli;
use super::surface;
use crate::launch::backend::{BackendId, OpenRequest, Placement, Surface};
use crate::launch::error::LaunchError;

pub async fn open(
    cli: &TerminalCli,
    req: &OpenRequest,
    timeouts: TerminalTimeouts,
) -> Result<Surface, LaunchError> {
    let folder = req.folder.to_string_lossy().into_owned();
    let session_uuid = req.session_uuid();
    let tag = surface::tag_for(session_uuid, &req.name);

    let command = crate::launch::fleet::seat_helpers::terminal_command(&req.program, &req.args)
        .map_err(LaunchError::Failed)?;

    let child = cli.spawn_xdg(&folder, &tag, &req.title, &command, &req.env)?;
    let window_pid = child.id().ok_or_else(|| {
        LaunchError::Failed("xdg-terminal-exec exited before reporting a pid".to_string())
    })?;
    surface::spawn_reaper(child);

    if !wait_for_window(cli, window_pid, timeouts).await {
        eprintln!(
            "mike spawn: window {window_pid} did not appear in `hyprctl clients` within \
             {:?}; continuing — the channel check decides whether the launch worked",
            timeouts.window
        );
    }
    let agent_pid = surface::wait_for_child_pid(window_pid, timeouts.child_pid).await;

    Ok(Surface {
        backend: BackendId::Terminal,
        workspace_ref: None,
        tab_ref: None,
        surface_ref: window_pid.to_string(),
        pid: agent_pid,
        session_uuid,
        agent_state: None,
        cwd: Some(folder),
        actual_placement: placement_note(req.placement),
        reused: false,
        delivered: None,
        confirmed: false,
        prompt_detail: None,
    })
}

async fn wait_for_window(cli: &TerminalCli, pid: u32, timeouts: TerminalTimeouts) -> bool {
    let deadline = Instant::now() + timeouts.window;
    loop {
        if matches!(
            surface::find_by_pid(cli, pid, super::cli::DEFAULT_TIMEOUT).await,
            Ok(Some(_))
        ) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(timeouts.window_poll).await;
    }
}

fn placement_note(placement: Placement) -> String {
    match placement {
        Placement::Window => "window".to_string(),
        other => format!(
            "window (requested {}; the terminal backend places windows only)",
            other.as_str()
        ),
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
mod tests {
    use super::*;

    #[test]
    fn placement_note_is_always_window_with_a_note_when_something_else_was_asked() {
        assert_eq!(placement_note(Placement::Window), "window");
        assert!(placement_note(Placement::Tab).starts_with("window (requested tab"));
    }
}
