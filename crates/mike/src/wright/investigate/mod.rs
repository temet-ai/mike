use crate::store::persistence::web_repo::WebPersist as _;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio::process::Child;
use uuid::Uuid;

use crate::store::persistence::wright::{Inserted, NewInvestigation};

pub(crate) mod admission;
pub(crate) mod brief;
pub(crate) mod context;
pub(crate) mod finding;
mod finish;
pub(crate) mod home;
mod launch;
mod login;
pub(crate) mod process;
pub(crate) mod reconcile;
mod record;
pub(crate) mod refusal;
pub(crate) mod relay;
pub(crate) mod relay_inside;
pub(crate) mod sandbox;
mod slot;
pub(crate) mod specialist;
pub(crate) mod supervise;
pub(crate) mod worker;

#[cfg(test)]
mod admission_tests;
#[cfg(test)]
mod brief_tests;
#[cfg(test)]
mod context_tests;
#[cfg(test)]
mod finding_tests;
#[cfg(test)]
mod login_tests;
#[cfg(test)]
mod reconcile_tests;
#[cfg(test)]
mod relay_tests;
#[cfg(test)]
mod run_tests;
#[cfg(test)]
mod sandbox_tests;
#[cfg(test)]
mod test_support;

pub(crate) use context::EngineerContext;
use home::RunDir;
use relay::RelayServer;
pub(crate) use supervise::RunEnd;
pub use worker::spawn_engineer_worker;

const READ_TOOLS: [&str; 4] = ["Read", "Glob", "Grep", "Bash"];
const REPAIR_TOOLS: [&str; 6] = ["Read", "Glob", "Grep", "Bash", "Edit", "Write"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunMode {
    Investigate,
    Repair {
        worktree: PathBuf,
        agent: &'static str,
    },
    Answer,
}

impl RunMode {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            RunMode::Investigate => "investigate",
            RunMode::Repair { .. } => "repair",
            RunMode::Answer => "answer",
        }
    }

    pub(crate) fn from_code(code: &str) -> Option<RunMode> {
        match code {
            "investigate" => Some(RunMode::Investigate),
            "repair" => Some(RunMode::Repair {
                worktree: PathBuf::new(),
                agent: specialist::for_part(""),
            }),
            "answer" => Some(RunMode::Answer),
            _ => None,
        }
    }

    pub(crate) fn output_file(&self) -> &'static str {
        match self {
            RunMode::Investigate => finding::FINDING_FILE,
            RunMode::Repair { .. } => "repair.json",
            RunMode::Answer => "answer.json",
        }
    }

    pub(crate) fn agent(&self) -> &'static str {
        match self {
            RunMode::Repair { agent, .. } => agent,
            RunMode::Investigate | RunMode::Answer => specialist::WRIGHT_ENGINEER,
        }
    }

    pub(crate) fn tools(&self) -> &'static [&'static str] {
        match self {
            RunMode::Repair { .. } => &REPAIR_TOOLS,
            RunMode::Investigate | RunMode::Answer => &READ_TOOLS,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RunRequest {
    pub mode: RunMode,
    pub problem_id: Option<i64>,
    pub followup_id: Option<i64>,
    pub brief: String,
    pub inputs: Vec<(String, Value)>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum StartRefused {
    #[error("the wright toggle is off")]
    Disabled,
    #[error("the machine is busy: {0}")]
    Overloaded(String),
    #[error("another engineer run is live")]
    Busy,
    #[error("{started} engineer runs started today; the daily limit is {limit}")]
    DailyLimit { started: i64, limit: i32 },
    #[error("the engineer is unavailable: {0}")]
    Unavailable(String),
    #[error("the Claude API refused the last run; no run starts before {until}")]
    Limited {
        until: chrono::DateTime<chrono::Utc>,
    },
    #[error("engineer run {investigation_id} could not start: {reason}")]
    Launch {
        investigation_id: i64,
        reason: String,
    },
}

pub(crate) struct RunHandle {
    pub(crate) investigation_id: i64,
    pub(crate) session_uuid: Uuid,
    pub(crate) mode: RunMode,
    pub(crate) deadline: DateTime<Utc>,
    pub(crate) out_dir: PathBuf,
    problem_id: Option<i64>,
    followup_id: Option<i64>,
    dir: RunDir,
    work: PathBuf,
    child: Child,
    root: i32,
    relay: RelayServer,
    ctx: EngineerContext,
}

#[derive(Debug, Clone)]
pub(crate) struct RunOutcome {
    pub investigation_id: i64,
    pub session_uuid: Uuid,
    pub end: RunEnd,
    pub out_dir: PathBuf,
    pub output: Result<Value, String>,
    pub finding_id: Option<i64>,
}

fn unavailable(e: impl std::fmt::Display) -> StartRefused {
    StartRefused::Unavailable(e.to_string())
}

async fn claim(
    ctx: &EngineerContext,
    req: &RunRequest,
    session: Uuid,
) -> Result<(i64, DateTime<Utc>), StartRefused> {
    reconcile::reconcile(&ctx.persist, &ctx.machine)
        .await
        .map_err(unavailable)?;
    let row = NewInvestigation {
        mode: req.mode.code(),
        problem_id: req.problem_id,
        session_uuid: session,
        host: &ctx.machine.host,
        machine_uuid: ctx.machine.uuid,
        owner_pid: i32::try_from(std::process::id()).unwrap_or(i32::MAX),
        followup_id: req.followup_id,
    };
    match ctx.persist.web().insert_wright_run(&row).await {
        Ok(Inserted::Started {
            id, deadline_at, ..
        }) => Ok((id, deadline_at)),
        Ok(Inserted::Busy) => Err(StartRefused::Busy),
        Ok(Inserted::DailyLimit { started, limit }) => {
            Err(StartRefused::DailyLimit { started, limit })
        }
        Err(e) => Err(unavailable(e)),
    }
}

pub(crate) async fn limited_until(ctx: &EngineerContext) -> Option<chrono::DateTime<chrono::Utc>> {
    match ctx.persist.web().wright_engineer_blocked_until().await {
        Ok(until) => until.filter(|until| *until > chrono::Utc::now()),
        Err(e) => {
            tracing::warn!(error = %e, "wright: the Claude limit could not be read; the run starts");
            None
        }
    }
}

pub(crate) async fn start_run(
    ctx: &EngineerContext,
    req: RunRequest,
) -> Result<RunHandle, StartRefused> {
    if !ctx.toggles.load().wright {
        return Err(StartRefused::Disabled);
    }
    if let Some(reason) = admission::machine_busy(&ctx.paths.locks()) {
        return Err(StartRefused::Overloaded(reason));
    }
    if let Some(until) = limited_until(ctx).await {
        return Err(StartRefused::Limited { until });
    }
    let session = Uuid::new_v4();
    let (id, deadline) = claim(ctx, &req, session).await?;
    let dir = RunDir::new(&ctx.paths.runs, id);
    let start = launch::Start {
        id,
        session,
        deadline,
        dir: &dir,
    };
    match launch::launch(ctx, &req, &start).await {
        Ok(launched) => Ok(RunHandle {
            investigation_id: id,
            session_uuid: session,
            mode: req.mode,
            deadline,
            out_dir: dir.output.clone(),
            problem_id: req.problem_id,
            followup_id: req.followup_id,
            dir,
            work: launched.work,
            child: launched.child,
            root: launched.root,
            relay: launched.relay,
            ctx: ctx.clone(),
        }),
        Err(reason) => {
            dir.remove_home();
            let _ = ctx
                .persist
                .web()
                .end_wright_run(id, "failed", None, Some(&reason))
                .await;
            Err(StartRefused::Launch {
                investigation_id: id,
                reason,
            })
        }
    }
}

pub(crate) async fn wait(handle: RunHandle) -> RunOutcome {
    let RunHandle {
        investigation_id,
        session_uuid,
        mode,
        deadline,
        out_dir,
        problem_id,
        followup_id,
        dir,
        work,
        mut child,
        root,
        relay,
        ctx,
        ..
    } = handle;
    let end = supervise::supervise(&ctx, investigation_id, &mut child, root, deadline).await;
    relay.stop();
    dir.remove_home();
    let ended = finish::Ended {
        id: investigation_id,
        mode: &mode,
        out: &out_dir,
        work: &work,
        problem_id,
        followup_id,
    };
    let settled = finish::settle(&ctx, &ended, &end).await;
    RunOutcome {
        investigation_id,
        session_uuid,
        end,
        out_dir,
        output: settled.output,
        finding_id: settled.finding_id,
    }
}
