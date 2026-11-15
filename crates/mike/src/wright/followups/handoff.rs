use crate::store::persistence::web_repo::WebPersist as _;
use std::time::Duration;

use serde::Deserialize;
use uuid::Uuid;

use super::FollowupError;
use super::context::{FollowupContext, Place, Target};
use super::prompt::fixer_prompt;
use super::quiet::quiet;
use super::runner::{Cmd, CommandRunner, Program, output};
use crate::store::persistence::wright::{FollowupLimits, FollowupRow};

const SPAWN_TIMEOUT: Duration = Duration::from_secs(3 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Handoff {
    FixerLive(usize),
    Capped,
    NothingFiled,
    NoQuietMachine(Vec<String>),
    HandedOff {
        id: i64,
        session: Uuid,
        machine: String,
    },
}

#[derive(Debug, Deserialize)]
struct Spawned {
    session: Option<Uuid>,
    #[serde(default)]
    opened: bool,
    reason: Option<String>,
}

pub(crate) async fn hand_off<R, S>(
    ctx: &FollowupContext<R, S>,
    limits: &FollowupLimits,
) -> Result<Handoff, FollowupError>
where
    R: CommandRunner,
{
    let web = ctx.persist.web();
    let live = web.live_wright_handoffs().await?;
    if !live.is_empty() {
        return Ok(Handoff::FixerLive(live.len()));
    }
    if web.wright_handoffs_today().await? >= i64::from(limits.daily_handoffs) {
        return Ok(Handoff::Capped);
    }
    let Some(row) = web.next_wright_handoff().await? else {
        return Ok(Handoff::NothingFiled);
    };
    let target = match quiet_target(ctx, &limits.fixers).await {
        Ok(target) => target,
        Err(reasons) => return Ok(Handoff::NoQuietMachine(reasons)),
    };
    let session = spawn(&ctx.runner, &limits.repo, &row, &target).await?;
    if !web
        .mark_wright_followup_handed_off(row.id, session, &target.machine)
        .await?
    {
        return Err(FollowupError::Output(format!(
            "fixer session {session} started for follow-up {}, which was no longer filed",
            row.id
        )));
    }
    Ok(Handoff::HandedOff {
        id: row.id,
        session,
        machine: target.machine,
    })
}

async fn quiet_target<R, S>(
    ctx: &FollowupContext<R, S>,
    fixers: &[crate::store::persistence::wright::Fixer],
) -> Result<Target, Vec<String>>
where
    R: CommandRunner,
{
    let mut reasons = Vec::new();
    for target in ctx.here.targets(fixers) {
        match quiet(&ctx.runner, &ctx.here, &target).await {
            Ok(()) => return Ok(target),
            Err(reason) => reasons.push(format!("{}: {reason}", target.machine)),
        }
    }
    if reasons.is_empty() {
        reasons.push("no fixer machine can be reached from here".to_owned());
    }
    Err(reasons)
}

pub(crate) fn spawn_cmd(repo: &str, row: &FollowupRow, number: i32, target: &Target) -> Cmd {
    let name = format!("wright-fix-{}", row.id);
    let goal = format!("Fix {repo}#{number}");
    let args = [
        "spawn",
        "open",
        &target.folder,
        "--json",
        "--role",
        "wright-fixer",
        "--session-name",
        &name,
        "--goal",
        &goal,
        "--prompt-file",
        "-",
    ];
    let cmd = match &target.place {
        Place::Local => Cmd::new(Program::Mike, &args),
        Place::Remote(host) => Cmd::remote(host, Program::Mike, &args),
    };
    cmd.with_stdin(fixer_prompt(repo, number))
        .with_timeout(SPAWN_TIMEOUT)
}

pub(crate) fn parse_spawned(stdout: &str) -> Result<Uuid, String> {
    let line = stdout
        .lines()
        .map(str::trim)
        .rev()
        .find(|line| line.starts_with('{'))
        .ok_or_else(|| format!("spawn printed {stdout:?}, with no JSON object"))?;
    let spawned: Spawned = serde_json::from_str(line)
        .map_err(|e| format!("spawn printed {line:?}, which is not its JSON result: {e}"))?;
    match spawned.session {
        Some(session) if spawned.opened => Ok(session),
        _ => Err(format!(
            "spawn did not open a session: {}",
            spawned.reason.as_deref().unwrap_or(line)
        )),
    }
}

async fn spawn<R: CommandRunner>(
    runner: &R,
    repo: &str,
    row: &FollowupRow,
    target: &Target,
) -> Result<Uuid, FollowupError> {
    let number = row.issue_number.ok_or_else(|| {
        FollowupError::Output(format!(
            "follow-up {} is filed with no issue number",
            row.id
        ))
    })?;
    let cmd = spawn_cmd(repo, row, number, target);
    let out = output(runner, &cmd).await?;
    parse_spawned(&out.stdout).map_err(|reason| {
        FollowupError::Output(format!(
            "`{}` on {} exited {:?}: {reason}; stderr: {}",
            cmd.line(),
            target.machine,
            out.code,
            out.stderr.trim()
        ))
    })
}
