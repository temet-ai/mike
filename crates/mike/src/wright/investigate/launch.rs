use crate::store::persistence::web_repo::WebPersist as _;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use chrono::{DateTime, Utc};
use tokio::process::Child;
use uuid::Uuid;

use super::context::{EngineerContext, Program};
use super::home::{self, HomeInputs, RunDir};
use super::relay::{RelayServer, RelayTargets};
use super::sandbox::{self, SandboxPlan, WorkAccess};
use super::{RunMode, RunRequest, slot};

pub(crate) struct Launched {
    pub(crate) child: Child,
    pub(crate) root: i32,
    pub(crate) relay: RelayServer,
    pub(crate) work: PathBuf,
}

pub(crate) fn claude_args(req: &RunRequest, session: Uuid) -> Vec<OsString> {
    [
        format!("{}/claude", sandbox::INSIDE_BIN),
        "-p".to_owned(),
        req.brief.clone(),
        "--session-id".to_owned(),
        session.to_string(),
        "--strict-mcp-config".to_owned(),
        "--mcp-config".to_owned(),
        home::inside_mcp_config(),
        "--permission-mode".to_owned(),
        "dontAsk".to_owned(),
        "--tools".to_owned(),
        req.mode.tools().join(","),
        "--agent".to_owned(),
        req.mode.agent().to_owned(),
        "--setting-sources".to_owned(),
        "user,project".to_owned(),
        "--output-format".to_owned(),
        "json".to_owned(),
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

fn command(program: &Program, req: &RunRequest, session: Uuid) -> Vec<OsString> {
    match program {
        Program::Claude => sandbox::relay_command(claude_args(req, session)),
        #[cfg(test)]
        Program::Script(argv) => argv.iter().map(OsString::from).collect(),
    }
}

async fn workspace(ctx: &EngineerContext, mode: &RunMode) -> Result<(PathBuf, WorkAccess), String> {
    if let RunMode::Repair { worktree, .. } = mode {
        if !worktree.is_absolute() || !worktree.is_dir() {
            return Err(format!(
                "the repair worktree {} is not an existing absolute directory",
                worktree.display()
            ));
        }
        return Ok((worktree.clone(), WorkAccess::ReadWrite));
    }
    if ctx.paths.refresh_slot {
        slot::refresh(&ctx.paths.repo, &ctx.paths.slot).await?;
    }
    Ok((ctx.paths.slot.clone(), WorkAccess::ReadOnly))
}

fn log_file(path: &Path) -> Result<std::fs::File, String> {
    std::fs::File::create(path).map_err(|e| format!("cannot create {}: {e}", path.display()))
}

fn spawn(bwrap: &Path, plan: &SandboxPlan, dir: &RunDir) -> Result<Child, String> {
    tokio::process::Command::new(bwrap)
        .args(sandbox::bwrap_args(plan))
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .stdout(log_file(&dir.stdout)?)
        .stderr(log_file(&dir.stderr)?)
        .process_group(0)
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("bwrap could not start: {e}"))
}

pub(crate) struct Start<'a> {
    pub(crate) id: i64,
    pub(crate) session: Uuid,
    pub(crate) deadline: DateTime<Utc>,
    pub(crate) dir: &'a RunDir,
}

fn plan(
    ctx: &EngineerContext,
    req: &RunRequest,
    start: &Start<'_>,
    work: (PathBuf, WorkAccess),
) -> Result<SandboxPlan, String> {
    let masked = sandbox::env_files(&work.0)
        .map_err(|e| format!("cannot list the .env files in {}: {e}", work.0.display()))?;
    let mut home_files = home::write_home(
        start.dir,
        &HomeInputs {
            credentials: &ctx.paths.credentials,
            deadline: start.deadline,
            tools: req.mode.tools(),
        },
    )?;
    home_files.extend(home::instruction_dirs(&ctx.paths.repo));
    Ok(SandboxPlan {
        system: sandbox::system_layout(),
        work: work.0,
        work_access: work.1,
        masked,
        input_dir: start.dir.input.clone(),
        output_dir: start.dir.output.clone(),
        home_files,
        socket: start.dir.socket.clone(),
        binaries: ctx.paths.binaries.clone(),
        env: sandbox::base_env(start.id),
        command: command(&ctx.program, req, start.session),
    })
}

fn relay(ctx: &EngineerContext, start: &Start<'_>) -> Result<RelayServer, String> {
    RelayServer::start(
        &start.dir.socket,
        RelayTargets {
            run_id: start.id,
            proxy: ctx.upstream.proxy.clone(),
            inspect: ctx.upstream.inspect.clone(),
            client: ctx.upstream.client.clone(),
        },
    )
}

async fn record_running(ctx: &EngineerContext, id: i64, child: &mut Child) -> Result<i32, String> {
    let root = child
        .id()
        .and_then(|pid| i32::try_from(pid).ok())
        .ok_or("bwrap exited before its pid was read")?;
    match ctx.persist.web().mark_wright_run_running(id, root).await {
        Ok(true) => Ok(root),
        Ok(false) => {
            Err("the run's row stopped being `starting` before the sandbox was recorded".to_owned())
        }
        Err(e) => Err(format!("the sandbox could not be recorded: {e}")),
    }
}

pub(crate) async fn launch(
    ctx: &EngineerContext,
    req: &RunRequest,
    start: &Start<'_>,
) -> Result<Launched, String> {
    start.dir.create()?;
    home::write_inputs(start.dir, &req.inputs)?;
    std::fs::write(start.dir.input.join("brief.txt"), &req.brief)
        .map_err(|e| format!("cannot write the brief: {e}"))?;
    let work = workspace(ctx, &req.mode).await?;
    let work_dir = work.0.clone();
    let plan = plan(ctx, req, start, work)?;
    let relay = relay(ctx, start)?;
    let mut child = match spawn(&ctx.paths.bwrap, &plan, start.dir) {
        Ok(child) => child,
        Err(e) => {
            relay.stop();
            return Err(e);
        }
    };
    match record_running(ctx, start.id, &mut child).await {
        Ok(root) => Ok(Launched {
            child,
            root,
            relay,
            work: work_dir,
        }),
        Err(e) => {
            if let Some(pid) = child.id().and_then(|pid| i32::try_from(pid).ok()) {
                super::process::kill_tree(pid);
            }
            relay.stop();
            Err(format!("{e}; the sandbox was killed"))
        }
    }
}
