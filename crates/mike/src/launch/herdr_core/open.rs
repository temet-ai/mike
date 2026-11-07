use std::collections::BTreeMap;
use std::time::Duration;

use uuid::Uuid;

use super::cli::{DEFAULT_TIMEOUT, HcCli, argv, str_field};
use super::place::{self, Place};
use super::prompt;
use super::terminal::{
    self, LAUNCHED_LABEL, NAME_LABEL, RUN_LABEL, SEAT_LABEL, SESSION_LABEL, SPACE_LABEL, TAB_LABEL,
};
use crate::launch::backend::{OpenRequest, PaneRequest, Surface};
use crate::launch::error::LaunchError;

const START_WAIT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(500);
const DIALOG_WAIT: Duration = Duration::from_secs(30);
const DIALOG_TEXT: &str = "Loading development channels";

pub(super) async fn open(
    cli: &HcCli,
    caller: Option<&str>,
    req: &OpenRequest,
) -> Result<Surface, LaunchError> {
    let folder = req.folder.to_string_lossy().into_owned();
    let placed = place::place(cli, caller, &folder, req.run.as_ref()).await;
    let session = session_uuid_from_args(&req.args);
    let env = with_caller_path(&req.env, std::env::var("PATH").ok());
    let command = agent_command(&req.program, &req.args, &env)?;
    let labels = agent_labels(&placed, req, session);
    let address = start(cli, &folder, &labels, &command).await?;
    if req
        .args
        .iter()
        .any(|a| a == crate::launch::DEV_CHANNELS_FLAG)
    {
        confirm_channels_dialog(cli, &address).await;
    }
    let state = wait_reported(cli, &address).await;
    let prompted = match &req.prompt {
        Some(text) => Some(prompt::deliver(cli, &address, text, req.prompt_wait_ms).await?),
        None => None,
    };
    let current = terminal::get(cli, &address).await?.ok_or_else(|| {
        LaunchError::Failed(format!(
            "herdr-core terminal {address} ended right after it started"
        ))
    })?;
    let mut surface = current.surface(&placed.placement, placed.reused);
    surface.session_uuid = session.or(surface.session_uuid);
    surface.agent_state = prompted
        .as_ref()
        .and_then(|p| p.agent_state.clone())
        .or(surface.agent_state)
        .or(state);
    surface.delivered = prompted.as_ref().map(|p| p.delivered);
    surface.confirmed = prompted.as_ref().is_some_and(|p| p.confirmed);
    surface.prompt_detail = prompted.and_then(|p| p.detail);
    Ok(surface)
}

pub(super) async fn open_pane(cli: &HcCli, req: &PaneRequest) -> Result<Surface, LaunchError> {
    let folder = req.folder.to_string_lossy().into_owned();
    let placed = place::place(cli, None, &folder, Some(&req.run)).await;
    let mut labels = place_labels(&placed);
    labels.push((RUN_LABEL, req.run.run_id.clone()));
    if let Some(seat) = &req.run.seat {
        labels.push((SEAT_LABEL, seat.clone()));
    }
    let command = vec!["sh".to_string(), "-c".to_string(), req.command.clone()];
    let address = start(cli, &folder, &labels, &command).await?;
    let current = terminal::get(cli, &address).await?.ok_or_else(|| {
        LaunchError::Failed(format!(
            "herdr-core terminal {address} ended right after it started"
        ))
    })?;
    Ok(current.surface(&placed.placement, placed.reused))
}

fn place_labels(placed: &Place) -> Vec<(&'static str, String)> {
    vec![
        (SPACE_LABEL, placed.space.clone()),
        (TAB_LABEL, placed.tab.clone()),
    ]
}

pub(super) fn agent_labels(
    placed: &Place,
    req: &OpenRequest,
    session: Option<Uuid>,
) -> Vec<(&'static str, String)> {
    let mut labels = place_labels(placed);
    labels.push((LAUNCHED_LABEL, "1".to_string()));
    labels.push((NAME_LABEL, req.name.clone()));
    if let Some(session) = session {
        labels.push((SESSION_LABEL, session.to_string()));
    }
    if let Some(run) = &req.run {
        labels.push((RUN_LABEL, run.run_id.clone()));
        if let Some(seat) = &run.seat {
            labels.push((SEAT_LABEL, seat.clone()));
        }
    }
    labels
}

pub(super) fn run_argv(
    folder: &str,
    labels: &[(&'static str, String)],
    command: &[String],
) -> Vec<String> {
    let mut args = argv(&[
        "run",
        "--bg",
        "--keep",
        "--json",
        "--no-parent",
        "--cwd",
        folder,
    ]);
    for (key, value) in labels {
        args.push("--label".to_string());
        args.push(format!("{key}={value}"));
    }
    args.push("--".to_string());
    args.extend(command.iter().cloned());
    args
}

async fn start(
    cli: &HcCli,
    folder: &str,
    labels: &[(&'static str, String)],
    command: &[String],
) -> Result<String, LaunchError> {
    let started = cli
        .hc_json(&run_argv(folder, labels, command), DEFAULT_TIMEOUT)
        .await?;
    str_field(&started, "address")
        .map(str::to_string)
        .ok_or_else(|| LaunchError::Malformed {
            command: "hc run".to_string(),
            detail: format!("no `address` in {started}"),
        })
}

pub(super) fn with_caller_path(
    env: &BTreeMap<String, String>,
    path: Option<String>,
) -> BTreeMap<String, String> {
    let mut env = env.clone();
    if let Some(path) = path.filter(|p| !p.is_empty()) {
        env.entry("PATH".to_string()).or_insert(path);
    }
    env
}

pub(super) fn agent_command(
    program: &str,
    args: &[String],
    env: &BTreeMap<String, String>,
) -> Result<Vec<String>, LaunchError> {
    let mut direct: Vec<String> = Vec::with_capacity(env.len() + args.len() + 2);
    if !env.is_empty() {
        direct.push("env".to_string());
        direct.extend(env.iter().map(|(k, v)| format!("{k}={v}")));
    }
    direct.push(program.to_string());
    direct.extend(args.iter().cloned());
    let setup = crate::launch::fleet::seat_helpers::setup(args).map_err(LaunchError::Failed)?;
    Ok(match setup {
        None => direct,
        Some(setup) => {
            let line = direct.iter().map(|a| shell_quote(a)).collect::<Vec<_>>();
            vec![
                "sh".to_string(),
                "-c".to_string(),
                format!("{} && exec {}", setup.command, line.join(" ")),
            ]
        }
    })
}

pub(super) fn shell_quote(arg: &str) -> String {
    let safe = arg
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_./:=,@".contains(c));
    if safe && !arg.is_empty() {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

pub(super) fn session_uuid_from_args(args: &[String]) -> Option<Uuid> {
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

async fn wait_reported(cli: &HcCli, address: &str) -> Option<String> {
    let deadline = tokio::time::Instant::now() + START_WAIT;
    loop {
        match terminal::get(cli, address).await {
            Ok(Some(t)) if t.status.is_some() => return t.status,
            Ok(Some(t)) if t.live => {}
            _ => return None,
        }
        if tokio::time::Instant::now() >= deadline {
            tracing::info!(
                address,
                "mike spawn: the herdr-core agent reported no status within the start wait"
            );
            return None;
        }
        tokio::time::sleep(POLL).await;
    }
}

async fn confirm_channels_dialog(cli: &HcCli, address: &str) {
    let deadline = tokio::time::Instant::now() + DIALOG_WAIT;
    while tokio::time::Instant::now() < deadline {
        let screen = read_screen(cli, address).await;
        if screen.contains(DIALOG_TEXT) {
            press_enter_on_dialog(cli, address).await;
            return;
        }
        tokio::time::sleep(POLL).await;
    }
    log_dialog_not_observed(address);
}

async fn read_screen(cli: &HcCli, address: &str) -> String {
    cli.hc_text(&argv(&["read", address]), DEFAULT_TIMEOUT)
        .await
        .unwrap_or_default()
}

async fn press_enter_on_dialog(cli: &HcCli, address: &str) {
    let pressed = cli
        .hc_json(
            &argv(&["keys", "--json", address, "enter"]),
            DEFAULT_TIMEOUT,
        )
        .await;
    match pressed {
        Ok(_) => log_dialog_confirmed(address),
        Err(error) => log_dialog_enter_failed(address, &error),
    }
}

fn log_dialog_confirmed(address: &str) {
    tracing::info!(
        address,
        "mike spawn: confirmed the development-channels dialog"
    );
}

fn log_dialog_enter_failed(address: &str, error: &LaunchError) {
    tracing::warn!(
        address,
        %error,
        "mike spawn: saw the development-channels dialog but failed to press Enter"
    );
}

fn log_dialog_not_observed(address: &str) {
    tracing::info!(
        address,
        "mike spawn: development-channels dialog was not observed within the wait budget"
    );
}
