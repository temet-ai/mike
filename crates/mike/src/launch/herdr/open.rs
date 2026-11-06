use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::Value;

use super::channels_dialog;
use super::cli::{DEFAULT_TIMEOUT, HerdrCli, str_field};
use super::prompt::{self, PromptOutcome};
use super::run_layout;
use super::surface::{PaneRef, agent_session_uuid, pane_at_shell_prompt, pane_pid};
use crate::launch::backend::{BackendId, OpenRequest, Placement, RunSeat, SplitDirection, Surface};
use crate::launch::error::LaunchError;

const AGENT_START_WAIT_MS: &str = "60000";
const AGENT_START_TIMEOUT: Duration = Duration::from_secs(75);
const PANE_BUSY_CODE: &str = "agent_pane_busy";
const PANE_BUSY_RETRY_BUDGET: Duration = Duration::from_secs(20);
const PANE_BUSY_RETRY_INTERVAL: Duration = Duration::from_millis(250);

struct Target {
    pane: PaneRef,
    placement: String,
    reused: bool,
    pane_created: bool,
}

pub(super) async fn open(cli: &HerdrCli, req: &OpenRequest) -> Result<Surface, LaunchError> {
    if req.program != "claude" {
        return Err(LaunchError::Unsupported(format!(
            "herdr starts `claude` from the pane's PATH itself; the program override `{}` cannot be honoured",
            req.program
        )));
    }
    let folder = req.folder.to_string_lossy().into_owned();
    let env = env_args(&req.env);
    let target = match &req.run {
        Some(run) => run_target(cli, run, &folder, &env).await?,
        None => folder_target(cli, req, &folder, &env).await?,
    };
    let wants_channels = req
        .args
        .iter()
        .any(|a| a == crate::launch::DEV_CHANNELS_FLAG);
    let (started, _) = tokio::join!(
        agent_start_when_shell_ready(cli, req, &target),
        confirm_channels_dialog(cli, &target.pane.pane, wants_channels),
    );
    let (agent_state, session_uuid) = match started {
        Ok(v) => v,
        Err(err) => return Err(fail_agent_start(cli, &target, err).await),
    };
    let prompted = match &req.prompt {
        Some(text) => Some(prompt::deliver(cli, &req.name, text, req.prompt_wait_ms).await?),
        None => None,
    };
    let pid = pane_pid(cli, &target.pane.pane).await;
    Ok(Surface {
        backend: BackendId::Herdr,
        workspace_ref: Some(target.pane.workspace),
        tab_ref: Some(target.pane.tab),
        surface_ref: target.pane.pane,
        pid,
        session_uuid,
        agent_state: prompted
            .as_ref()
            .and_then(|p| p.agent_state.clone())
            .or(agent_state),
        cwd: Some(folder),
        actual_placement: target.placement,
        reused: target.reused,
        delivered: prompted.as_ref().map(|p| p.delivered),
        confirmed: prompted.as_ref().is_some_and(PromptOutcome::is_confirmed),
        prompt_detail: prompted.and_then(|p| p.detail),
    })
}

async fn run_target(
    cli: &HerdrCli,
    run: &RunSeat,
    folder: &str,
    env: &[String],
) -> Result<Target, LaunchError> {
    let placed = run_layout::place(cli, run, folder, env).await?;
    Ok(Target {
        pane: placed.pane,
        placement: placed.placement,
        reused: !placed.created_workspace,
        pane_created: true,
    })
}

async fn folder_target(
    cli: &HerdrCli,
    req: &OpenRequest,
    folder: &str,
    env: &[String],
) -> Result<Target, LaunchError> {
    match worktree_main_root(&req.folder).await {
        Some(main_root) => open_worktree(cli, &main_root, folder, &req.env).await,
        None => match find_pane_in_folder(cli, folder).await? {
            None => create_workspace(cli, folder, env).await,
            Some(existing) => place_in_existing(cli, req, folder, env, existing).await,
        },
    }
}

fn env_args(env: &BTreeMap<String, String>) -> Vec<String> {
    env.iter()
        .flat_map(|(k, v)| ["--env".to_string(), format!("{k}={v}")])
        .collect()
}

async fn find_pane_in_folder(cli: &HerdrCli, folder: &str) -> Result<Option<PaneRef>, LaunchError> {
    let result = cli
        .run_json(&args(&["pane", "list"]), DEFAULT_TIMEOUT)
        .await?;
    let panes = result
        .get("panes")
        .and_then(Value::as_array)
        .ok_or_else(|| LaunchError::Malformed {
            command: "herdr pane list".into(),
            detail: format!("no `panes` array in {result}"),
        })?;
    Ok(panes
        .iter()
        .find(|p| str_field(p, "cwd") == Some(folder))
        .and_then(PaneRef::from_value))
}

async fn worktree_main_root(folder: &std::path::Path) -> Option<std::path::PathBuf> {
    match crate::launch::worktree::detect(folder).await {
        Ok(crate::launch::WorktreeKind::Linked { main_root }) => main_root,
        _ => None,
    }
}

async fn confirm_channels_dialog(
    cli: &HerdrCli,
    pane: &str,
    wants_channels: bool,
) -> Option<channels_dialog::ChannelsDialogOutcome> {
    if !wants_channels {
        return None;
    }
    Some(channels_dialog::confirm(cli, pane).await)
}

async fn open_worktree(
    cli: &HerdrCli,
    main_root: &std::path::Path,
    folder: &str,
    env: &BTreeMap<String, String>,
) -> Result<Target, LaunchError> {
    let argv = args(&[
        "worktree",
        "open",
        "--cwd",
        &main_root.to_string_lossy(),
        "--path",
        folder,
        "--trust-repository",
        "--no-focus",
    ]);
    let result = cli.run_json(&argv, DEFAULT_TIMEOUT).await?;
    let pane = result
        .get("root_pane")
        .and_then(PaneRef::from_value)
        .ok_or_else(|| malformed("herdr worktree open", &result))?;
    if !env.is_empty() {
        export_env(cli, &pane.pane, env).await?;
    }
    Ok(Target {
        pane,
        placement: "worktree".to_string(),
        reused: false,
        pane_created: true,
    })
}

async fn export_env(
    cli: &HerdrCli,
    pane: &str,
    env: &BTreeMap<String, String>,
) -> Result<(), LaunchError> {
    let command = env
        .iter()
        .map(|(k, v)| format!("export {k}='{}'", v.replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join("; ");
    cli.run_json(&args(&["pane", "run", pane, &command]), DEFAULT_TIMEOUT)
        .await
        .map(|_| ())
}

async fn create_workspace(
    cli: &HerdrCli,
    folder: &str,
    env: &[String],
) -> Result<Target, LaunchError> {
    let mut argv = args(&["workspace", "create", "--cwd", folder]);
    argv.extend_from_slice(env);
    argv.push("--no-focus".to_string());
    let result = cli.run_json(&argv, DEFAULT_TIMEOUT).await?;
    let pane = result
        .get("root_pane")
        .and_then(PaneRef::from_value)
        .ok_or_else(|| malformed("herdr workspace create", &result))?;
    Ok(Target {
        pane,
        placement: "workspace".to_string(),
        reused: false,
        pane_created: true,
    })
}

async fn place_in_existing(
    cli: &HerdrCli,
    req: &OpenRequest,
    folder: &str,
    env: &[String],
    existing: PaneRef,
) -> Result<Target, LaunchError> {
    match req.placement {
        Placement::Tab => create_tab(cli, &existing.workspace, folder, env, "tab").await,
        Placement::Split(dir) => split_pane(cli, &existing, dir, folder, env).await,
        Placement::Window | Placement::Panel => {
            let requested = req.placement.as_str();
            if env.is_empty() && pane_at_shell_prompt(cli, &existing.pane).await? {
                return Ok(Target {
                    pane: existing,
                    placement: format!("{requested} (workspace pane at its shell prompt)"),
                    reused: true,
                    pane_created: false,
                });
            }
            let note = format!("tab (requested {requested}; a fresh pane carries the environment)");
            create_tab(cli, &existing.workspace, folder, env, &note).await
        }
    }
}

async fn create_tab(
    cli: &HerdrCli,
    workspace: &str,
    folder: &str,
    env: &[String],
    placement: &str,
) -> Result<Target, LaunchError> {
    let mut argv = args(&["tab", "create", "--workspace", workspace, "--cwd", folder]);
    argv.extend_from_slice(env);
    argv.push("--no-focus".to_string());
    let result = cli.run_json(&argv, DEFAULT_TIMEOUT).await?;
    let pane = result
        .get("root_pane")
        .and_then(PaneRef::from_value)
        .ok_or_else(|| malformed("herdr tab create", &result))?;
    Ok(Target {
        pane,
        placement: placement.to_string(),
        reused: true,
        pane_created: true,
    })
}

async fn split_pane(
    cli: &HerdrCli,
    existing: &PaneRef,
    dir: SplitDirection,
    folder: &str,
    env: &[String],
) -> Result<Target, LaunchError> {
    let herdr_dir = match dir {
        SplitDirection::Left | SplitDirection::Right => "right",
        SplitDirection::Up | SplitDirection::Down => "down",
    };
    let placement = if herdr_dir == dir.as_str() {
        format!("split:{herdr_dir}")
    } else {
        format!("split:{herdr_dir} (requested {})", dir.as_str())
    };
    let mut argv = args(&[
        "pane",
        "split",
        "--pane",
        &existing.pane,
        "--direction",
        herdr_dir,
        "--cwd",
        folder,
    ]);
    argv.extend_from_slice(env);
    argv.push("--no-focus".to_string());
    let result = cli.run_json(&argv, DEFAULT_TIMEOUT).await?;
    let pane = result
        .get("pane")
        .and_then(PaneRef::from_value)
        .ok_or_else(|| malformed("herdr pane split", &result))?;
    Ok(Target {
        pane,
        placement,
        reused: true,
        pane_created: true,
    })
}

async fn agent_start_when_shell_ready(
    cli: &HerdrCli,
    req: &OpenRequest,
    target: &Target,
) -> Result<(Option<String>, Option<uuid::Uuid>), LaunchError> {
    super::browser_guard::prepare(cli, &req.args, &target.pane.pane).await?;
    let deadline = tokio::time::Instant::now() + PANE_BUSY_RETRY_BUDGET;
    loop {
        match agent_start(cli, req, &target.pane.pane).await {
            Err(err)
                if target.pane_created
                    && err.backend_code() == Some(PANE_BUSY_CODE)
                    && tokio::time::Instant::now() < deadline =>
            {
                tracing::debug!(pane = %target.pane.pane, "herdr pane still starting its shell; retrying agent start");
                tokio::time::sleep(PANE_BUSY_RETRY_INTERVAL).await;
            }
            other => return other,
        }
    }
}

async fn agent_start(
    cli: &HerdrCli,
    req: &OpenRequest,
    pane: &str,
) -> Result<(Option<String>, Option<uuid::Uuid>), LaunchError> {
    let mut argv = args(&[
        "agent",
        "start",
        &req.name,
        "--kind",
        "claude",
        "--pane",
        pane,
        "--timeout",
        AGENT_START_WAIT_MS,
        "--",
    ]);
    argv.extend(req.args.iter().cloned());
    match cli.run_json(&argv, AGENT_START_TIMEOUT).await {
        Ok(result) => {
            let agent = result.get("agent").unwrap_or(&Value::Null);
            Ok((
                str_field(agent, "agent_status").map(str::to_string),
                agent_session_uuid(agent),
            ))
        }
        Err(err) if err.backend_code() == Some("agent_not_ready") => {
            Ok(agent_state_after_not_ready(cli, &req.name).await)
        }
        Err(err) => Err(err),
    }
}

async fn agent_state_after_not_ready(
    cli: &HerdrCli,
    name: &str,
) -> (Option<String>, Option<uuid::Uuid>) {
    match cli
        .run_json(&args(&["agent", "get", name]), DEFAULT_TIMEOUT)
        .await
    {
        Ok(result) => {
            let agent = result.get("agent").unwrap_or(&result);
            (
                Some(
                    str_field(agent, "agent_status")
                        .unwrap_or("agent_not_ready")
                        .to_string(),
                ),
                agent_session_uuid(agent),
            )
        }
        Err(_) => (Some("agent_not_ready".to_string()), None),
    }
}

const PANE_TAIL_LINES: &str = "20";
const PANE_TAIL_WINDOW: usize = 20;

async fn fail_agent_start(cli: &HerdrCli, target: &Target, err: LaunchError) -> LaunchError {
    let pane = &target.pane.pane;
    let err = match pane_tail(cli, pane).await {
        Some(tail) => err.with_pane_output(tail),
        None => err,
    };
    if target.pane_created {
        close_created_pane(cli, pane).await;
    }
    err
}

async fn pane_tail(cli: &HerdrCli, pane: &str) -> Option<String> {
    let read_args = args(&[
        "pane",
        "read",
        pane,
        "--source",
        "recent",
        "--lines",
        PANE_TAIL_LINES,
        "--format",
        "text",
    ]);
    let text = cli.run_text(&read_args, DEFAULT_TIMEOUT).await.ok()?;
    last_non_empty_lines(&text)
}

fn last_non_empty_lines(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let end = lines.iter().rposition(|line| !line.trim().is_empty())? + 1;
    let start = end.saturating_sub(PANE_TAIL_WINDOW);
    Some(lines[start..end].join("\n"))
}

async fn close_created_pane(cli: &HerdrCli, pane: &str) {
    let _ = cli
        .run_json(&args(&["pane", "close", pane]), DEFAULT_TIMEOUT)
        .await;
}

fn args(parts: &[&str]) -> Vec<String> {
    super::cli::argv(parts)
}

fn malformed(command: &str, result: &Value) -> LaunchError {
    LaunchError::Malformed {
        command: command.to_string(),
        detail: format!("no pane ids in {result}"),
    }
}
