use std::time::Duration;

use uuid::Uuid;

use super::ledger::Ledger;
use super::{mcp_preflight, server};
use crate::api::launches::mcp_seen;
use crate::config::AppState;
use crate::launch::ServerOpenRequest;
use crate::store::persistence::launch::LaunchOutcome;

#[path = "seat_health_start.rs"]
mod start;

use start::Start;

pub(crate) const TOOLS_WAIT: Duration = Duration::from_secs(180);
const TOOLS_POLL: Duration = Duration::from_secs(5);
const FAILED_REASON: &str = "mike_tools_missing";
const START_FAILED: &str = "start_failed";
const REPLACED: &str = "seat_replaced";
const RELAUNCH_PROMPT: &str = "mike relaunched this seat because mike's MCP tools did not load the \
                               first time; they are passed to it explicitly now. Carry on with \
                               your brief from where you stopped.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Relaunch {
    pub(crate) folder: String,
    pub(crate) claude_args: Vec<String>,
    pub(crate) prompt: Option<String>,
    pub(crate) template_id: Option<i64>,
    pub(crate) run: Option<String>,
    pub(crate) seat: Option<String>,
    pub(crate) role: Option<String>,
    pub(crate) instructions: Option<String>,
    pub(crate) goal: Option<String>,
    pub(crate) worktree_ok: bool,
    pub(crate) proxy_env: bool,
    pub(crate) remote_control: bool,
    pub(crate) launch_id: Option<i64>,
    pub(crate) backend: Option<crate::launch::BackendId>,
}

impl Relaunch {
    pub(crate) fn from_request(req: &ServerOpenRequest) -> Self {
        Self {
            folder: req.folder.clone(),
            claude_args: req.claude_args.clone(),
            prompt: req.prompt.clone(),
            template_id: req.template_id,
            run: req.run.clone(),
            seat: req.seat.clone(),
            role: req.role.clone(),
            instructions: req.instructions.clone(),
            goal: req.goal.clone(),
            worktree_ok: req.worktree_ok,
            proxy_env: req.proxy_env,
            remote_control: req.remote_control,
            launch_id: None,
            backend: req.backend,
        }
    }
}

fn session_flag(arg: &str) -> bool {
    arg == "--session-id" || arg == "--resume"
}

fn dropped(arg: &str) -> bool {
    arg == "--fork-session"
        || arg.starts_with("--session-id=")
        || arg.starts_with("--resume=")
        || arg.starts_with("--mcp-config=")
}

pub(crate) fn corrected_args(args: &[String], session: Uuid, mcp_config: &str) -> Vec<String> {
    let mut out = vec!["--resume".to_string(), session.to_string()];
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if session_flag(arg) {
            iter.next();
            continue;
        }
        if !dropped(arg) {
            out.push(arg.clone());
        }
    }
    out.push(format!("--mcp-config={mcp_config}"));
    out
}

pub(crate) async fn wait_for_tools(session: Uuid, wait: Duration, poll: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        if mcp_seen::tools_listed(session) {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(poll).await;
    }
}

fn relaunch_request(
    relaunch: &Relaunch,
    session: Uuid,
    mcp_config: &str,
    start: Start,
) -> ServerOpenRequest {
    let (claude_args, prompt, template_id) = match start {
        Start::Resume => (
            corrected_args(&relaunch.claude_args, session, mcp_config),
            Some(RELAUNCH_PROMPT.to_string()),
            None,
        ),
        Start::Fresh => (
            start::fresh_args(&relaunch.claude_args, session, mcp_config),
            relaunch.prompt.clone(),
            relaunch.template_id,
        ),
    };
    ServerOpenRequest {
        folder: relaunch.folder.clone(),
        claude_args,
        prompt,
        run: relaunch.run.clone(),
        seat: relaunch.seat.clone(),
        session_name: None,
        role: relaunch.role.clone(),
        instructions: relaunch.instructions.clone(),
        goal: relaunch.goal.clone(),
        template_id,
        worktree_ok: relaunch.worktree_ok,
        trust: false,
        proxy_env: relaunch.proxy_env,
        remote_control: relaunch.remote_control,
        channels: false,
        backend: relaunch.backend,
        caller: crate::launch::backend_choice::CallerTerminal::default(),
    }
}

async fn relaunch_once(
    state: &AppState,
    relaunch: &Relaunch,
    (session, start): (Uuid, Start),
    tried: &mut Vec<String>,
) -> Result<Option<i64>, String> {
    match server::kill_for_server(state, session).await {
        Ok(report) => tried.push(format!(
            "closed the seat's pane (closed: {})",
            report.closed
        )),
        Err(e) => tried.push(format!("could not close the seat's pane: {e}")),
    }
    let Some(bin) = mcp_preflight::find_server() else {
        let looked = mcp_preflight::tried_line();
        tried.push(looked.clone());
        return Err(format!("the mike MCP config cannot be built: {looked}"));
    };
    let config = mcp_preflight::inline_config(&bin, Some(&state.config.inspect_listen_addr));
    tried.push(format!(
        "relaunched the seat once with {} {session} and --mcp-config naming {}",
        start.flag(),
        bin.display()
    ));
    let request = relaunch_request(relaunch, session, &config, start);
    server::open_unwatched(state.clone(), request)
        .await
        .map(|start| start.launch_id)
        .map_err(|e| e.to_string())
}

async fn needs_relaunch(session: Uuid) -> bool {
    if wait_for_tools(session, TOOLS_WAIT, TOOLS_POLL).await {
        return false;
    }
    if !mcp_seen::reporting() {
        tracing::warn!(session_id = %session, "no mike-mcp on this machine reports its tools yet; the seat's tools are not judged");
        return false;
    }
    true
}

async fn after_relaunch(
    relaunch: &Relaunch,
    session: Uuid,
    relaunched: Result<Option<i64>, String>,
) -> Option<(Option<i64>, String)> {
    match relaunched {
        Ok(id) if wait_for_tools(session, TOOLS_WAIT, TOOLS_POLL).await => {
            tracing::info!(session_id = %session, ?id, "a seat without mike's tools was relaunched and now has them");
            None
        }
        Ok(id) => Some((
            id.or(relaunch.launch_id),
            format!("the relaunched seat {session} did not list mike's tools either"),
        )),
        Err(why) => Some((relaunch.launch_id, why)),
    }
}

pub(crate) async fn watch(state: &AppState, relaunch: Relaunch, session: Uuid) {
    if !needs_relaunch(session).await {
        return;
    }
    if start::replaced(state, &relaunch, session).await {
        let why = format!("seat {session} was replaced before mike relaunched it");
        record_failure(state, relaunch.launch_id, REPLACED, &why).await;
        return;
    }
    let start = start::start_for(state, &relaunch, session).await;
    let mut tried = vec![format!(
        "waited {}s for session {session} to list mike's tools; it did not ({})",
        TOOLS_WAIT.as_secs(),
        match start {
            Start::Resume => "the session has a transcript or requests",
            Start::Fresh => "the session never started: no transcript and no request",
        }
    )];
    let relaunched = relaunch_once(state, &relaunch, (session, start), &mut tried).await;
    if let Some((launch_id, error)) = after_relaunch(&relaunch, session, relaunched).await {
        let failure = Failure {
            launch_id,
            tried,
            error,
            start,
        };
        give_up(state, &relaunch, session, failure).await;
    }
}

struct Failure {
    launch_id: Option<i64>,
    tried: Vec<String>,
    error: String,
    start: Start,
}

fn failed_outcome(reason: &str, error: &str) -> LaunchOutcome {
    LaunchOutcome {
        result: "failed".to_string(),
        reason: Some(reason.to_string()),
        opened: reason == FAILED_REASON,
        delivered: None,
        confirmed: false,
        channel_check: None,
        stderr: Some(crate::store::persistence::launch::StderrCapture::capture(
            error,
        )),
    }
}

async fn record_failure(state: &AppState, launch_id: Option<i64>, reason: &str, error: &str) {
    if let Some(persist) = state.persist() {
        Ledger::new(Some(persist.clone()))
            .record_outcome(launch_id, &failed_outcome(reason, error))
            .await;
    }
}

fn failure_kind(start: Start) -> &'static str {
    match start {
        Start::Resume => FAILED_REASON,
        Start::Fresh => START_FAILED,
    }
}

async fn give_up(state: &AppState, relaunch: &Relaunch, session: Uuid, failure: Failure) {
    let Failure {
        launch_id,
        tried,
        error,
        start,
    } = failure;
    record_failure(state, launch_id, failure_kind(start), &error).await;
    let replaced = start::replaced(state, relaunch, session).await;
    note_unrecovered(relaunch, session, replaced, &tried);
}

fn note_unrecovered(relaunch: &Relaunch, session: Uuid, replaced: bool, tried: &[String]) {
    if replaced {
        log_replaced(session);
        return;
    }
    log_undecided(relaunch, session, tried);
}

fn log_replaced(session: Uuid) {
    tracing::info!(session_id = %session, "a seat without mike's tools was replaced meanwhile");
}

fn log_undecided(relaunch: &Relaunch, session: Uuid, tried: &[String]) {
    let seat = relaunch.seat.as_deref().unwrap_or("-");
    tracing::warn!(session_id = %session, seat, ?tried, "a seat has no mike tools: the fleet supervisor decides its replacement");
}

#[cfg(test)]
#[path = "seat_health_tests.rs"]
mod tests;
