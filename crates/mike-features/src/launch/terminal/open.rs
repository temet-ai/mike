use std::time::Instant;

use uuid::Uuid;

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
    let session_uuid = session_uuid_from_args(&req.args);
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
    let claude_pid = surface::wait_for_child_pid(window_pid, timeouts.child_pid).await;

    Ok(Surface {
        backend: BackendId::Terminal,
        workspace_ref: None,
        tab_ref: None,
        surface_ref: window_pid.to_string(),
        pid: claude_pid,
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

fn session_uuid_from_args(args: &[String]) -> Option<Uuid> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--session-id" {
            return iter.next()?.parse().ok();
        }
        if let Some(value) = arg.strip_prefix("--session-id=") {
            return value.parse().ok();
        }
    }
    None
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
    fn session_uuid_from_args_reads_space_and_equals_forms() {
        let id = Uuid::new_v4();
        assert_eq!(
            session_uuid_from_args(&["--session-id".into(), id.to_string()]),
            Some(id)
        );
        assert_eq!(
            session_uuid_from_args(&[format!("--session-id={id}")]),
            Some(id)
        );
        assert_eq!(session_uuid_from_args(&["--resume".into()]), None);
    }

    #[test]
    fn placement_note_is_always_window_with_a_note_when_something_else_was_asked() {
        assert_eq!(placement_note(Placement::Window), "window");
        assert!(placement_note(Placement::Tab).starts_with("window (requested tab"));
    }
}
