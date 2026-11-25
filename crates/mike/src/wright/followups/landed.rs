use crate::store::persistence::web_repo::WebPersist as _;
use std::collections::HashMap;

use super::FollowupError;
use super::context::{FollowupContext, Place};
use super::runner::{Cmd, CommandRunner, Program, checked};
use crate::store::persistence::wright::{FollowupLimits, FollowupRow};
use crate::wright::repair::batch_git::{LOG_FORMAT, parse_log};

const TRAILER: &str = "Wright-Followup:";
const OPEN_READ: i64 = 500;
const GREP: &str = "--grep=^Wright-Followup:";

pub(crate) fn landed_followups(stdout: &str) -> HashMap<i64, String> {
    let mut landed = HashMap::new();
    for commit in parse_log(stdout) {
        for line in commit.body.lines() {
            let named = line.strip_prefix(TRAILER);
            if let Some(Ok(id)) = named.map(|id| id.trim().parse::<i64>()) {
                landed.entry(id).or_insert_with(|| commit.sha.clone());
            }
        }
    }
    landed
}

async fn read_main<R, S>(
    ctx: &FollowupContext<R, S>,
    limits: &FollowupLimits,
) -> Result<HashMap<i64, String>, FollowupError>
where
    R: CommandRunner,
{
    let targets = ctx.here.fix_targets(&limits.fixers, &limits.fix_host);
    let Some(target) = targets.first() else {
        return Ok(HashMap::new());
    };
    let args = [
        "-C",
        &target.folder,
        "log",
        "--max-count=400",
        GREP,
        LOG_FORMAT,
        "origin/main",
    ];
    let cmd = match &target.place {
        Place::Local => Cmd::new(Program::Git, &args),
        Place::Remote(host) => Cmd::remote(host, Program::Git, &args),
    };
    Ok(landed_followups(&checked(&ctx.runner, &cmd).await?))
}

async fn close_one<R, S>(
    ctx: &FollowupContext<R, S>,
    limits: &FollowupLimits,
    row: &FollowupRow,
    sha: &str,
) -> Result<(), FollowupError>
where
    R: CommandRunner,
{
    let reason = format!("fixed on main by {sha}, which names follow-up {}", row.id);
    let state = match row.issue_number {
        Some(number) => {
            let number = number.to_string();
            let comment = format!("Wright: {reason}.");
            let args = [
                "issue",
                "close",
                &number,
                "-R",
                &limits.repo,
                "--comment",
                &comment,
            ];
            checked(&ctx.runner, &Cmd::new(Program::Gh, &args)).await?;
            "fixed"
        }
        None => "dropped",
    };
    ctx.persist
        .web()
        .mark_wright_followup_closed(row.id, state, &reason)
        .await?;
    Ok(())
}

pub(crate) async fn close_landed<R, S>(
    ctx: &FollowupContext<R, S>,
    limits: &FollowupLimits,
) -> Result<Vec<(i64, String)>, FollowupError>
where
    R: CommandRunner,
{
    let web = ctx.persist.web();
    let mut waiting = web.tracked_wright_followups().await?;
    waiting.retain(|row| matches!(row.state.as_str(), "filed" | "stuck"));
    waiting.extend(web.wright_followups_to_file(OPEN_READ).await?);
    if waiting.is_empty() {
        return Ok(Vec::new());
    }
    let landed = read_main(ctx, limits).await?;
    let mut failed = Vec::new();
    for row in &waiting {
        let Some(sha) = landed.get(&row.id) else {
            continue;
        };
        if let Err(e) = close_one(ctx, limits, row, sha).await {
            failed.push((row.id, e.to_string()));
        }
    }
    Ok(failed)
}
