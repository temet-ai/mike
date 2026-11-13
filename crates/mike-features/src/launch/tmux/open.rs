use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use uuid::Uuid;

use super::channels_dialog;
use super::cli::{DEFAULT_TIMEOUT, TmuxCli, argv};
use super::prompt;
use super::seat_line;
use super::surface::{self, session_name_from_agent};
use crate::launch::backend::{BackendId, OpenRequest, Surface};
use crate::launch::error::LaunchError;

const READY_TIMEOUT: Duration = Duration::from_secs(20);
const READY_POLL: Duration = Duration::from_millis(300);
const SEND_TIMEOUT: Duration = Duration::from_secs(15);

struct Target {
    pane_id: String,
    session: String,
    tab_ref: String,
    placement: String,
    reused: bool,
}

pub async fn open(cli: &TmuxCli, req: &OpenRequest) -> Result<Surface, LaunchError> {
    let folder = req.folder.to_string_lossy().into_owned();
    let env_flags = env_args(&req.env);
    let session_uuid = session_uuid_from_args(&req.args);

    let target = match &req.run_tab {
        Some(window) => split_in_window(cli, window, &folder, &env_flags).await?,
        None => {
            let name = session_name_from_agent(&req.name);
            new_session(cli, &name, &folder, &env_flags, req).await?
        }
    };
    if let Some(id) = session_uuid {
        record_session_uuid(cli, &target.pane_id, id).await;
    }

    send_command(cli, &target.pane_id, req).await?;
    wait_ready(cli, &target.pane_id).await;
    if req
        .args
        .iter()
        .any(|a| a == crate::launch::DEV_CHANNELS_FLAG)
    {
        channels_dialog::confirm(cli, &target.pane_id).await;
    }

    let (delivered, confirmed, detail) = match &req.prompt {
        Some(text) => {
            let outcome = prompt::deliver(cli, &target.pane_id, text).await?;
            (Some(outcome.delivered), outcome.confirmed, outcome.detail)
        }
        None => (None, false, None),
    };
    let pid = surface::foreground_child_pid(cli, &target.pane_id).await;

    Ok(Surface {
        backend: BackendId::Tmux,
        workspace_ref: Some(target.session),
        tab_ref: Some(target.tab_ref),
        surface_ref: target.pane_id,
        pid,
        session_uuid,
        agent_state: None,
        cwd: Some(folder),
        actual_placement: target.placement,
        reused: target.reused,
        delivered,
        confirmed,
        prompt_detail: detail,
    })
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

fn env_args(env: &BTreeMap<String, String>) -> Vec<String> {
    env.iter()
        .flat_map(|(k, v)| ["-e".to_string(), format!("{k}={v}")])
        .collect()
}

async fn new_session(
    cli: &TmuxCli,
    name: &str,
    folder: &str,
    env_flags: &[String],
    req: &OpenRequest,
) -> Result<Target, LaunchError> {
    let mut args = argv(&["new-session", "-d", "-s", name, "-c", folder]);
    args.extend_from_slice(env_flags);
    args.extend(argv(&["-P", "-F", "#{pane_id}\u{1f}#{window_index}"]));
    let text = cli.run_text(&args, DEFAULT_TIMEOUT).await?;
    let (pane_id, window_index) = split_created(&text, "tmux new-session")?;
    Ok(Target {
        tab_ref: format!("{name}:{window_index}"),
        placement: placement_note(req),
        pane_id,
        session: name.to_string(),
        reused: false,
    })
}

async fn split_in_window(
    cli: &TmuxCli,
    window: &str,
    folder: &str,
    env_flags: &[String],
) -> Result<Target, LaunchError> {
    let mut args = argv(&["split-window", "-t", window, "-c", folder]);
    args.extend_from_slice(env_flags);
    args.extend(argv(&[
        "-P",
        "-F",
        "#{session_name}\u{1f}#{pane_id}\u{1f}#{window_index}",
    ]));
    let text = cli.run_text(&args, DEFAULT_TIMEOUT).await?;
    let mut fields = text.trim().split('\u{1f}');
    let session = fields
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| malformed("tmux split-window", &text))?;
    let pane_id = fields
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| malformed("tmux split-window", &text))?;
    let window_index = fields.next().unwrap_or("0");
    let tab_ref = format!("{session}:{window_index}");
    cli.run_ok(
        &argv(&["select-layout", "-t", &tab_ref, "tiled"]),
        DEFAULT_TIMEOUT,
    )
    .await?;
    Ok(Target {
        pane_id: pane_id.to_string(),
        session: session.to_string(),
        tab_ref,
        placement: "split (run window, tiled)".to_string(),
        reused: true,
    })
}

fn split_created(text: &str, command: &str) -> Result<(String, String), LaunchError> {
    let mut fields = text.trim().split('\u{1f}');
    let pane_id = fields
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| malformed(command, text))?
        .to_string();
    let window_index = fields.next().unwrap_or("0").to_string();
    Ok((pane_id, window_index))
}

fn malformed(command: &str, text: &str) -> LaunchError {
    LaunchError::Malformed {
        command: command.to_string(),
        detail: format!("expected a pane id, got {text:?}"),
    }
}

fn placement_note(req: &OpenRequest) -> String {
    let requested = req.placement.as_str();
    match req.placement {
        crate::launch::backend::Placement::Window | crate::launch::backend::Placement::Tab => {
            requested
        }
        _ => format!("window (requested {requested}; tmux has no per-folder reuse to split)"),
    }
}

async fn record_session_uuid(cli: &TmuxCli, pane: &str, session: Uuid) {
    let args = argv(&[
        "set-option",
        "-p",
        "-t",
        pane,
        "@mike_session",
        &session.to_string(),
    ]);
    if let Err(e) = cli.run_ok(&args, DEFAULT_TIMEOUT).await {
        eprintln!("mike spawn: could not record the session id on the tmux pane: {e}");
    }
}

async fn send_command(cli: &TmuxCli, pane: &str, req: &OpenRequest) -> Result<(), LaunchError> {
    let env = seat_line::seat_env(|key| std::env::var(key).ok(), &req.env);
    let line = seat_line::command_line(&req.program, &req.args, &env)?;
    cli.run_ok(
        &argv(&["send-keys", "-t", pane, &line, "Enter"]),
        SEND_TIMEOUT,
    )
    .await
}

async fn wait_ready(cli: &TmuxCli, pane: &str) {
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        match surface::pane_current_command(cli, pane).await {
            Some(cmd) if cmd == "claude" => return,
            _ if Instant::now() >= deadline => {
                eprintln!(
                    "mike spawn: pane {pane} did not become `claude` within {READY_TIMEOUT:?}; \
                     continuing — the channel check decides whether the launch worked"
                );
                return;
            }
            _ => tokio::time::sleep(READY_POLL).await,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
pub mod tests {
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
    fn env_args_are_dash_e_pairs_in_key_order() {
        let mut env = BTreeMap::new();
        env.insert("B".to_string(), "2".to_string());
        env.insert("A".to_string(), "1".to_string());
        assert_eq!(env_args(&env), vec!["-e", "A=1", "-e", "B=2"]);
    }

    #[test]
    fn split_created_reads_pane_and_window() {
        let (pane, window) = split_created("%7\u{1f}2\n", "tmux new-session").unwrap();
        assert_eq!(pane, "%7");
        assert_eq!(window, "2");
    }
}
