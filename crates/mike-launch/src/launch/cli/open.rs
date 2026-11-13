use std::io::Read;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::channel::{self, ChannelBudget};
use super::delivery::{self, Delivery, PROMPT_WAIT_MS};
use super::folder;
use super::open_gates;
use super::open_outcome::{channel_source, failed_outcome, opened_output, success_outcome};
use super::open_record;
use super::prompt_turn;
use super::registration;
use super::run_seat;
use super::runner::Runner;
use crate::launch::{
    AssembleOptions, BackendId, ChannelCheck, LaunchError, LaunchOutput, LaunchSpec, OpenRequest,
    Placement, SplitLaunchArgs, WorktreeKind, brief, split_launch_args, worktree,
};

#[path = "fleet_admission.rs"]
pub mod fleet_admission;

pub const MCP_MISSING: &str = "mike_mcp_missing";

#[derive(Debug)]
pub struct OpenFailure {
    pub output: Option<LaunchOutput>,
    pub error: LaunchError,
}

pub type OpenResult = Result<LaunchOutput, OpenFailure>;

pub const OPEN_FAILED: &str = "open_failed";

pub fn exit_code(result: &OpenResult) -> i32 {
    match result {
        Ok(output) => output.exit_code(),
        Err(failure) => failure.error.exit_code(),
    }
}

pub struct Prepared {
    pub split: SplitLaunchArgs,
    pub folder: PathBuf,
    pub repo_root: Option<PathBuf>,
    pub worktree: bool,
    pub prompt: Option<String>,
    pub output: LaunchOutput,
}

impl Prepared {
    pub fn claude_args(&self) -> &[String] {
        &self.split.claude_args
    }

    pub fn default_role(&mut self, discipline: &str) {
        if self.split.role.is_none() {
            self.split.role = Some(discipline.to_string());
        }
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    pub fn backend(&self) -> BackendId {
        self.output.backend
    }
}

pub struct Started {
    admission: Option<fleet_admission::Admission>,
    output: LaunchOutput,
    spec: LaunchSpec,
    delivery: Delivery,
    request: OpenRequest,
    launch_id: Option<i64>,
    repo_root: Option<PathBuf>,
}

impl Started {
    pub fn launch_id(&self) -> Option<i64> {
        self.launch_id
    }

    pub fn session(&self) -> Option<Uuid> {
        self.spec.session
    }
}

pub async fn execute(
    runner: &Runner<'_>,
    tokens: Vec<String>,
    stdin: &mut (dyn Read + Send),
) -> OpenResult {
    let prepared = prepare(runner, tokens, stdin).await?;
    launch(runner, prepared).await
}

pub fn fail(output: LaunchOutput, error: LaunchError, reason: &str) -> OpenFailure {
    OpenFailure {
        output: Some(LaunchOutput {
            reason: Some(reason.to_string()),
            ..output
        }),
        error,
    }
}

fn bare(error: LaunchError) -> OpenFailure {
    OpenFailure {
        output: None,
        error,
    }
}

fn strip_no_trust(tokens: Vec<String>) -> (Vec<String>, bool) {
    let mut no_trust = false;
    let mut kept = Vec::with_capacity(tokens.len());
    let mut verbatim = false;
    for token in tokens {
        if !verbatim && token == "--no-trust" {
            no_trust = true;
            continue;
        }
        if token == "--" {
            verbatim = true;
        }
        kept.push(token);
    }
    (kept, no_trust)
}

fn take_folder(split: &mut SplitLaunchArgs) -> Result<String, LaunchError> {
    if split.claude_args.is_empty() || split.claude_args[0].starts_with('-') {
        return Err(LaunchError::Usage(
            "the folder must come first: mike spawn open <folder> [spawn flags] [claude args...]"
                .into(),
        ));
    }
    Ok(split.claude_args.remove(0))
}

fn skeleton(backend: BackendId, folder: &str, split: &SplitLaunchArgs) -> LaunchOutput {
    LaunchOutput {
        backend,
        workspace: None,
        folder: folder.to_string(),
        surface: None,
        session: None,
        reused: false,
        adopted: false,
        opened: false,
        delivered: None,
        confirmed: false,
        channel_check: ChannelCheck::NoSessionId,
        brief_mode: None,
        run: split.run.clone(),
        seat: split.seat.clone(),
        reason: None,
    }
}

pub async fn prepare(
    runner: &Runner<'_>,
    tokens: Vec<String>,
    stdin: &mut (dyn Read + Send),
) -> Result<Prepared, OpenFailure> {
    let (tokens, no_trust) = strip_no_trust(tokens);
    let mut split = split_launch_args(tokens).map_err(bare)?;
    let raw_folder = take_folder(&mut split).map_err(bare)?;
    let mut output = skeleton(runner.backend.id(), &raw_folder, &split);
    if runner.backend.id() == BackendId::None {
        return Err(fail(output, LaunchError::BackendNone, "backend_none"));
    }
    let folder = folder::resolve_from_env(&raw_folder)
        .map_err(|e| fail(output.clone(), e, "no_such_directory"))?;
    output.folder = folder.to_string_lossy().into_owned();
    let kind = worktree::check(&folder, split.worktree_ok)
        .await
        .map_err(|e| fail(output.clone(), e, "linked_worktree"))?;
    if let crate::launch::WorktreeKind::Linked {
        main_root: Some(main_root),
    } = &kind
        && split.carry_config
    {
        let files = worktree::carry_over_files(runner.config.worktree_config.as_deref());
        open_gates::report_carry_over(&worktree::carry_over(main_root, &folder, &files));
    }
    let target = open_gates::trust_gate(runner, &folder, &split, no_trust, &output).await?;
    super::mcp_preflight::apply(runner, &folder, &mut split)
        .map_err(|e| fail(output.clone(), e, MCP_MISSING))?;
    open_gates::surface_cap(runner, &output).await?;
    let prompt = match &split.prompt_file {
        Some(source) => Some(
            brief::read_prompt(source, stdin)
                .map_err(|e| fail(output.clone(), e, "prompt_file"))?,
        ),
        None => None,
    };
    Ok(Prepared {
        repo_root: (kind != WorktreeKind::NotRepo).then_some(target),
        worktree: matches!(kind, WorktreeKind::Linked { .. }),
        split,
        folder,
        prompt,
        output,
    })
}

pub fn with_operator_args(mut split: SplitLaunchArgs, operator: &[String]) -> SplitLaunchArgs {
    let mut args = operator.to_vec();
    args.append(&mut split.claude_args);
    split.claude_args = args;
    split
}

async fn launch(runner: &Runner<'_>, p: Prepared) -> OpenResult {
    let started = start(runner, p).await?;
    finish(runner, started).await
}

pub async fn start(runner: &Runner<'_>, p: Prepared) -> Result<Started, OpenFailure> {
    let admission = fleet_admission::admit(runner, p.split.run.as_deref())
        .await
        .map_err(|error| fail(p.output.clone(), error, "fleet_admission"))?;
    let mut split = with_operator_args(p.split.clone(), &runner.config.claude_args);
    crate::launch::seat::before_open(&runner.ledger, &mut split)
        .await
        .map_err(|e| fail(p.output.clone(), e, "fleet_settings"))?;
    let opts = AssembleOptions {
        program: "claude".to_string(),
        proxy: runner.proxy.as_ref(),
        brief_inline_max: runner.config.brief_inline_max,
        briefs_dir: &runner.briefs_dir,
        brief_notes: runner.ledger.available(),
        brief_by_mail: runner.ledger.mail_briefs(),
    };
    let mut spec = LaunchSpec::assemble(split, p.prompt.clone(), &opts)
        .map_err(|e| fail(p.output.clone(), e, "usage"))?;
    let prompt_in_argv = runner.backend.capabilities().prompt_in_argv;
    let mut delivery = Delivery::split(&spec, prompt_in_argv);
    let placed = run_seat::resolve(runner, &p.split).await;
    let launch_id = open_record::record_start(runner, &p, &spec, &delivery).await;
    if admission.is_some() && launch_id.is_none() {
        return Err(fail(
            p.output,
            LaunchError::Failed(
                "fleet launch was not durably recorded; no backend open was attempted".into(),
            ),
            "fleet_ledger_unavailable",
        ));
    }
    if launch_id.is_none() && spec.brief.as_ref().is_some_and(|b| b.note.is_some()) {
        spec.demote_brief_to_file(&runner.briefs_dir)
            .map_err(|e| fail(p.output.clone(), e, "prompt_file"))?;
        delivery = Delivery::split(&spec, prompt_in_argv);
    }
    let request = OpenRequest {
        folder: p.folder.clone(),
        placement: p.split.placement.unwrap_or(Placement::Tab),
        env: spec.env.clone(),
        title: spec.title.clone(),
        run_tab: placed.run_tab,
        run: placed.run,
        program: spec.program.clone(),
        args: delivery.args.clone(),
        name: spec.agent_name.clone(),
        prompt: delivery.prompt.clone(),
        prompt_wait_ms: PROMPT_WAIT_MS,
    };
    Ok(Started {
        admission,
        output: p.output,
        spec,
        delivery,
        request,
        launch_id,
        repo_root: p.repo_root,
    })
}

async fn record_open_failure(
    runner: &Runner<'_>,
    launch_id: Option<i64>,
    fleet: bool,
    error: &LaunchError,
) {
    if fleet {
        fleet_admission::record_unconfirmed(runner, launch_id, error).await;
    } else {
        let outcome = failed_outcome(OPEN_FAILED, error);
        runner.ledger.record_outcome(launch_id, &outcome).await;
    }
}

pub async fn finish(runner: &Runner<'_>, started: Started) -> OpenResult {
    let Started {
        admission,
        output,
        spec,
        delivery,
        request,
        launch_id,
        repo_root,
    } = started;
    let watch = prompt_turn::TurnWatch::new(spec.session, delivery.prompt.as_deref());
    let surface = match runner.backend.open(&request).await {
        Ok(surface) => surface,
        Err(e) => {
            record_open_failure(runner, launch_id, admission.is_some(), &e).await;
            return Err(fail(output, e, OPEN_FAILED));
        }
    };
    crate::launch::local_root::remember_opened(
        launch_id,
        request.run.as_ref().map(|run| run.run_id.as_str()),
        spec.session,
        &surface,
    );
    if let Err(error) =
        fleet_admission::record_opened(runner, admission, launch_id, &request, &surface).await
    {
        fleet_admission::record_unconfirmed(runner, launch_id, &error).await;
        return Err(OpenFailure {
            output: Some(opened_output(output, &spec, &surface, &delivery)),
            error,
        });
    }
    if let Some(session) = spec.session {
        registration::register_opened_surface(
            runner.inspect_addr.as_deref(),
            session,
            &request.folder,
            repo_root.as_deref(),
            surface.pid,
        )
        .await;
    }
    let mut output = opened_output(output, &spec, &surface, &delivery);
    let captured = request.env.contains_key("HTTPS_PROXY");
    prompt_turn::confirm_from_capture(&runner.ledger, watch.as_ref(), captured, &mut output).await;
    let typed = prompt_turn::Typed {
        agent: &spec.agent_name,
        prompt: delivery.prompt.as_deref(),
        captured,
    };
    prompt_turn::resend_if_lost(runner, watch.as_ref(), typed, &mut output).await;
    let check = channel::check(
        &runner.ledger,
        spec.session,
        &channel_source(&request.folder, spec.session),
        &ChannelBudget::from_config(runner.config),
    )
    .await;
    output.channel_check = check;
    if check.failed() {
        output.reason = Some("channel_timeout".to_string());
    }
    if let Some(note) = super::brief_delivery::deliver_note_brief(runner, &spec, check).await {
        output.delivered = Some(note.delivered());
        output.confirmed = note.confirmed();
    }
    let unconfirmed = spec.prompt.is_some() && !output.confirmed;
    if unconfirmed {
        output.reason = Some(delivery::reason(&output).to_string());
    }
    runner
        .ledger
        .record_outcome(launch_id, &success_outcome(&output, unconfirmed))
        .await;
    if unconfirmed {
        return Err(OpenFailure {
            error: delivery::unconfirmed_error(&output, &surface),
            output: Some(output),
        });
    }
    Ok(output)
}

#[cfg(any(test, feature = "test-seams"))]
#[path = "fleet_admission_tests.rs"]
pub mod admission_tests;
