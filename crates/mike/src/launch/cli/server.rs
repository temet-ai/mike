use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;

use uuid::Uuid;

use super::doctor::{self, DoctorReport};
use super::kill::{self, KillReport};
use super::ledger::Ledger;
use super::open::{self, OPEN_FAILED};
use super::proxy_env::resolve_proxy_env;
use super::runner::Runner;
use super::seat_health::Relaunch;
use crate::launch::backend_choice::{self, BackendChoice, CallerTerminal};
use crate::launch::fleet::{brief_delivery, brief_version};
use crate::launch::{BackendId, LaunchError, TerminalBackend, TrustGate, brief};
use crate::state::AppState;

pub(crate) struct ServerOpenRequest {
    pub(crate) folder: String,
    pub(crate) claude_args: Vec<String>,
    pub(crate) prompt: Option<String>,
    pub(crate) run: Option<String>,
    pub(crate) seat: Option<String>,
    pub(crate) session_name: Option<String>,
    pub(crate) role: Option<String>,
    pub(crate) instructions: Option<String>,
    pub(crate) goal: Option<String>,
    pub(crate) template_id: Option<i64>,
    pub(crate) worktree_ok: bool,
    pub(crate) trust: bool,
    pub(crate) proxy_env: bool,
    pub(crate) remote_control: bool,
    pub(crate) channels: bool,
    pub(crate) backend: Option<BackendId>,
    pub(crate) caller: CallerTerminal,
}

pub(crate) struct ServerOpenStart {
    pub(crate) launch_id: Option<i64>,
    pub(crate) session: Option<Uuid>,
    pub(crate) backend: BackendId,
    pub(crate) backend_reason: String,
}

fn build_tokens(req: &ServerOpenRequest) -> Vec<String> {
    let mut tokens = Vec::with_capacity(req.claude_args.len() + 8);
    tokens.push(req.folder.clone());
    if let Some(run) = &req.run {
        tokens.push("--run".to_string());
        tokens.push(run.clone());
    }
    if let Some(seat) = &req.seat {
        tokens.push("--seat".to_string());
        tokens.push(seat.clone());
    }
    if let Some(session_name) = &req.session_name {
        tokens.push("--session-name".to_string());
        tokens.push(session_name.clone());
    }
    if let Some(role) = &req.role {
        tokens.push("--role".to_string());
        tokens.push(role.clone());
    }
    if let Some(instructions) = &req.instructions {
        tokens.push("--instructions".to_string());
        tokens.push(instructions.clone());
    }
    if let Some(goal) = &req.goal {
        tokens.push("--goal".to_string());
        tokens.push(goal.clone());
    }
    if let Some(template) = req.template_id {
        tokens.push("--template-id".to_string());
        tokens.push(template.to_string());
    }
    if req.worktree_ok {
        tokens.push("--worktree-ok".to_string());
    }
    if req.trust {
        tokens.push("--trust".to_string());
    }
    if !req.proxy_env {
        tokens.push("--no-proxy-env".to_string());
    }
    if !req.remote_control {
        tokens.push("--no-remote-control".to_string());
    }
    if !req.channels {
        tokens.push("--no-channels".to_string());
    }
    tokens.extend(req.claude_args.iter().cloned());
    tokens
}

fn attach_prompt_file(
    mut tokens: Vec<String>,
    prompt: Option<&str>,
) -> Result<(Vec<String>, Option<tempfile::NamedTempFile>), LaunchError> {
    let Some(prompt) = prompt else {
        return Ok((tokens, None));
    };
    let mut file = tempfile::Builder::new()
        .prefix(".mike-spawn-prompt-")
        .tempfile()
        .map_err(|e| LaunchError::io("creating a temporary prompt file", e))?;
    file.as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|e| LaunchError::io("securing the temporary prompt file", e))?;
    file.write_all(prompt.as_bytes())
        .map_err(|e| LaunchError::io("writing the temporary prompt file", e))?;
    file.flush()
        .map_err(|e| LaunchError::io("flushing the temporary prompt file", e))?;
    tokens.push("--prompt-file".to_string());
    tokens.push(file.path().to_string_lossy().into_owned());
    Ok((tokens, Some(file)))
}

async fn resolve_backend(
    state: &AppState,
    explicit: Option<BackendId>,
    caller: &CallerTerminal,
) -> Result<(BackendChoice, Box<dyn TerminalBackend>), LaunchError> {
    backend_choice::resolve(&state.config.launch, explicit, caller).await
}

async fn configured_backend(state: &AppState) -> Result<Box<dyn TerminalBackend>, LaunchError> {
    let (_, backend) = resolve_backend(state, None, &CallerTerminal::default()).await?;
    Ok(backend)
}

async fn build_runner<'a>(
    state: &'a AppState,
    backend: &'a dyn TerminalBackend,
) -> Result<Runner<'a>, LaunchError> {
    let cwd = std::env::current_dir().ok();
    Ok(Runner {
        backend,
        ledger: Ledger::new(state.persist().cloned())
            .with_mail_briefs(state.feature_toggle_overlay.load().launch_brief)
            .with_queue(super::queue::LocalQueue::default_dir()),
        config: &state.config.launch,
        proxy: resolve_proxy_env(&state.config.launch, cwd.as_deref()),
        trust: TrustGate::from_home()?,
        briefs_dir: brief::default_briefs_dir()?,
        inspect_addr: Some(state.config.inspect_listen_addr.clone()),
        mcp_server: super::mcp_preflight::find_server(),
        backend_reason: None,
    })
}

async fn stamped_prompt(
    state: &AppState,
    (run, seat): (Option<&str>, Option<&str>),
    prompt: Option<&str>,
) -> Option<String> {
    let prompt = prompt?;
    let from_notes = match (state.persist(), run, seat) {
        (Some(persist), Some(run), Some(seat)) => {
            brief_delivery::first_message(persist, run, seat).await
        }
        _ => None,
    };
    let prompt = from_notes.as_deref().unwrap_or(prompt);
    let version = match (state.persist(), run) {
        (Some(persist), Some(run)) => brief_version::version_for_run(persist, run).await,
        _ => None,
    };
    Some(version.map_or_else(
        || prompt.to_string(),
        |v| brief_version::stamped(prompt, &v),
    ))
}

pub(crate) async fn open_for_server(
    state: AppState,
    req: ServerOpenRequest,
) -> Result<ServerOpenStart, LaunchError> {
    open_server(state, req, true).await
}

pub(crate) async fn open_unwatched(
    state: AppState,
    req: ServerOpenRequest,
) -> Result<ServerOpenStart, LaunchError> {
    open_server(state, req, false).await
}

async fn open_server(
    state: AppState,
    req: ServerOpenRequest,
    watch: bool,
) -> Result<ServerOpenStart, LaunchError> {
    let (choice, backend) = resolve_backend(&state, req.backend, &req.caller).await?;
    let mut runner = build_runner(&state, backend.as_ref()).await?;
    runner.backend_reason = Some(choice.reason.clone());
    let mut follow = watch.then(|| Relaunch::from_request(&req));
    let prompt = stamped_prompt(
        &state,
        (req.run.as_deref(), req.seat.as_deref()),
        req.prompt.as_deref(),
    )
    .await;
    let tokens = build_tokens(&req);
    let (tokens, prompt_file) = attach_prompt_file(tokens, prompt.as_deref())?;
    let prepared = open::prepare(&runner, tokens, &mut std::io::empty()).await;
    drop(prompt_file);
    let prepared = match prepared {
        Ok(prepared) => prepared,
        Err(failure) => return Err(failure.error),
    };
    let started = open::start(&runner, prepared).await.map_err(|f| f.error)?;
    let launch_id = started.launch_id();
    let session = started.session();
    let backend_id = runner.backend.id();
    if let Some(follow) = follow.as_mut() {
        follow.launch_id = launch_id;
        follow.backend = Some(choice.id);
    }
    let backend_reason = choice.reason.clone();
    spawn_finish(state, choice, started, follow);
    Ok(ServerOpenStart {
        launch_id,
        session,
        backend: backend_id,
        backend_reason,
    })
}

fn spawn_finish(
    state: AppState,
    choice: BackendChoice,
    started: open::Started,
    follow: Option<Relaunch>,
) {
    let tracker = state.tracker.clone();
    tracker.spawn(async move {
        let Some(backend) = choice.backend() else {
            tracing::warn!(backend = %choice.id, "mike spawn: backend disappeared before the launch could finish");
            return;
        };
        let session = started.session();
        let mut pane_never_opened = false;
        match build_runner(&state, backend.as_ref()).await {
            Ok(runner) => {
                if let Err(failure) = open::finish(&runner, started).await {
                    pane_never_opened = never_opened(&failure);
                    tracing::warn!(
                        error = %failure.error,
                        "mike spawn: a launch's background finish did not succeed"
                    );
                }
            }
            Err(e) => tracing::warn!(
                error = %e,
                "mike spawn: could not build a runner to finish the launch in the background"
            ),
        }
        match (follow, session) {
            (Some(_), Some(session)) if pane_never_opened => close_late_pane(&state, session).await,
            (Some(follow), Some(session)) => {
                super::seat_health::watch(&state, follow, session).await;
            }
            _ => {}
        }
    });
}

fn never_opened(failure: &open::OpenFailure) -> bool {
    failure
        .output
        .as_ref()
        .and_then(|output| output.reason.as_deref())
        == Some(OPEN_FAILED)
}

async fn close_late_pane(state: &AppState, session: Uuid) {
    let outcome = match kill_for_server(state, session).await {
        Ok(report) => format!("closed: {}", report.closed),
        Err(e) => format!("closing failed: {e}"),
    };
    tracing::warn!(session_id = %session, %outcome, "mike spawn: the seat's pane never opened; mike closed any late pane and does not relaunch the seat");
}

pub(crate) async fn kill_for_server(
    state: &AppState,
    session: Uuid,
) -> Result<KillReport, LaunchError> {
    let backend = configured_backend(state).await?;
    let runner = build_runner(state, backend.as_ref()).await?;
    kill::execute(&runner, session).await
}

pub(crate) async fn doctor_for_server(state: &AppState) -> Result<DoctorReport, LaunchError> {
    let backend = configured_backend(state).await?;
    let runner = build_runner(state, backend.as_ref()).await?;
    let env = super::doctor_env(&state.config)?;
    Ok(doctor::execute(&runner, &env).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_tokens_orders_flags_before_claude_args() {
        let req = ServerOpenRequest {
            folder: "/repo".to_string(),
            claude_args: vec!["--model".to_string(), "opus".to_string()],
            prompt: None,
            run: Some("r1".to_string()),
            seat: Some("worker-1".to_string()),
            session_name: Some("observer-1".to_string()),
            role: Some("observer".to_string()),
            instructions: Some("watch the fleet".to_string()),
            goal: Some("ship phase 1".to_string()),
            template_id: Some(7),
            worktree_ok: true,
            trust: true,
            proxy_env: false,
            remote_control: false,
            channels: true,
            backend: None,
            caller: CallerTerminal::default(),
        };
        assert_eq!(
            build_tokens(&req),
            vec![
                "/repo",
                "--run",
                "r1",
                "--seat",
                "worker-1",
                "--session-name",
                "observer-1",
                "--role",
                "observer",
                "--instructions",
                "watch the fleet",
                "--goal",
                "ship phase 1",
                "--template-id",
                "7",
                "--worktree-ok",
                "--trust",
                "--no-proxy-env",
                "--no-remote-control",
                "--model",
                "opus",
            ]
        );
    }

    #[test]
    fn build_tokens_defaults_omit_every_optional_flag() {
        let req = ServerOpenRequest {
            folder: "/repo".to_string(),
            claude_args: vec![],
            prompt: None,
            run: None,
            seat: None,
            session_name: None,
            role: None,
            instructions: None,
            goal: None,
            template_id: None,
            worktree_ok: false,
            trust: false,
            proxy_env: true,
            remote_control: true,
            channels: true,
            backend: None,
            caller: CallerTerminal::default(),
        };
        assert_eq!(build_tokens(&req), vec!["/repo"]);
    }

    #[test]
    fn attach_prompt_file_none_leaves_tokens_untouched() -> anyhow::Result<()> {
        let (tokens, file) = attach_prompt_file(vec!["/repo".to_string()], None)?;
        assert_eq!(tokens, vec!["/repo".to_string()]);
        assert!(file.is_none());
        Ok(())
    }

    #[test]
    fn attach_prompt_file_some_writes_a_0600_file_and_appends_the_flag() -> anyhow::Result<()> {
        let (tokens, file) = attach_prompt_file(vec!["/repo".to_string()], Some("do the thing"))?;
        let file = file.ok_or_else(|| anyhow::anyhow!("expected a temp file"))?;
        assert_eq!(tokens[0], "/repo");
        assert_eq!(tokens[1], "--prompt-file");
        assert_eq!(tokens[2], file.path().to_string_lossy());
        assert_eq!(std::fs::read_to_string(file.path())?, "do the thing");
        assert_eq!(
            std::fs::metadata(file.path())?.permissions().mode() & 0o777,
            0o600
        );
        let path = file.path().to_path_buf();
        drop(file);
        assert!(!path.exists(), "the temp file must be removed on drop");
        Ok(())
    }
}
