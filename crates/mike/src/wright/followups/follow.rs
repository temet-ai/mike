use crate::store::persistence::web_repo::WebPersist as _;

use serde::Deserialize;

use super::FollowupError;
use super::context::{FollowupContext, Place};
use super::runner::{Cmd, CommandRunner, Program, checked};
use super::sessions::FixerSessions;
use crate::store::persistence::wright::{FixBatchRow, FollowupLimits, FollowupRow};
use crate::wright::repair::batch_git::{LOG_FORMAT, parse_log, trailer_id};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    Fixed(String),
    Dropped(String),
    Open,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Followed {
    Fixed(String),
    Dropped(String),
    Working,
    Batched(String),
    Returned { state: String, why: String },
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct FollowReport {
    pub(crate) followed: Vec<(i64, Followed)>,
    pub(crate) failed: Vec<(i64, String)>,
}

#[derive(Debug, Deserialize)]
struct PullRef {
    number: i64,
    #[serde(default)]
    url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IssueState {
    state: String,
    #[serde(default)]
    state_reason: String,
    #[serde(default)]
    closed_by_pull_requests_references: Vec<PullRef>,
}

pub(crate) fn verdict(issue: &IssueState) -> Verdict {
    if !issue.state.eq_ignore_ascii_case("closed") {
        return Verdict::Open;
    }
    let reason = issue.state_reason.to_ascii_uppercase();
    if reason == "NOT_PLANNED" || reason == "DUPLICATE" {
        return Verdict::Dropped(format!(
            "the issue was closed as {}",
            reason.to_ascii_lowercase().replace('_', " ")
        ));
    }
    match issue.closed_by_pull_requests_references.first() {
        Some(pr) => Verdict::Fixed(format!("closed by pull request #{} {}", pr.number, pr.url)),
        None => Verdict::Fixed("the issue was closed as completed".to_owned()),
    }
}

pub(crate) fn parse_issue(stdout: &str) -> Result<IssueState, String> {
    serde_json::from_str(stdout)
        .map_err(|e| format!("`gh issue view` printed {stdout:?}, not the issue's state: {e}"))
}

pub(crate) async fn follow<R, S>(
    ctx: &FollowupContext<R, S>,
    limits: &FollowupLimits,
) -> Result<FollowReport, FollowupError>
where
    R: CommandRunner,
    S: FixerSessions,
{
    let mut report = FollowReport::default();
    for row in ctx.persist.web().tracked_wright_followups().await? {
        match follow_one(ctx, limits, &row).await {
            Ok(followed) => report.followed.push((row.id, followed)),
            Err(e) => report.failed.push((row.id, e.to_string())),
        }
    }
    Ok(report)
}

async fn issue<R: CommandRunner>(
    runner: &R,
    repo: &str,
    number: i32,
) -> Result<IssueState, FollowupError> {
    let number = number.to_string();
    let cmd = Cmd::new(
        Program::Gh,
        &[
            "issue",
            "view",
            &number,
            "-R",
            repo,
            "--json",
            "state,stateReason,closedByPullRequestsReferences",
        ],
    );
    let out = checked(runner, &cmd).await?;
    parse_issue(&out).map_err(FollowupError::Output)
}

pub(crate) fn answered_comment(children: &[i32]) -> String {
    let list = children
        .iter()
        .map(|n| format!("#{n}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Wright: every cause this root cause split into is closed ({list}), so this is answered."
    )
}

async fn close_answered_root<R, S>(
    ctx: &FollowupContext<R, S>,
    limits: &FollowupLimits,
    row: &FollowupRow,
    number: i32,
) -> Result<Option<String>, FollowupError>
where
    R: CommandRunner,
    S: FixerSessions,
{
    if row.kind != "root_cause" {
        return Ok(None);
    }
    let web = ctx.persist.web();
    let Some(children) = web.wright_followup_children_closed(row.id).await? else {
        return Ok(None);
    };
    let comment = answered_comment(&children);
    let issue_number = number.to_string();
    let cmd = Cmd::new(
        Program::Gh,
        &[
            "issue",
            "close",
            &issue_number,
            "-R",
            &limits.repo,
            "--comment",
            &comment,
        ],
    );
    checked(&ctx.runner, &cmd).await?;
    web.mark_wright_followup_closed(row.id, "fixed", &comment)
        .await?;
    Ok(Some(comment))
}

async fn follow_one<R, S>(
    ctx: &FollowupContext<R, S>,
    limits: &FollowupLimits,
    row: &FollowupRow,
) -> Result<Followed, FollowupError>
where
    R: CommandRunner,
    S: FixerSessions,
{
    let missing = |what: &str| {
        FollowupError::Output(format!("follow-up {} is handed off with no {what}", row.id))
    };
    let number = row.issue_number.ok_or_else(|| missing("issue number"))?;
    let web = ctx.persist.web();
    match verdict(&issue(&ctx.runner, &limits.repo, number).await?) {
        Verdict::Fixed(how) => {
            web.mark_wright_followup_closed(row.id, "fixed", &how)
                .await?;
            return Ok(Followed::Fixed(how));
        }
        Verdict::Dropped(how) => {
            web.mark_wright_followup_closed(row.id, "dropped", &how)
                .await?;
            return Ok(Followed::Dropped(how));
        }
        Verdict::Open => {}
    }
    if let Some(how) = close_answered_root(ctx, limits, row, number).await? {
        return Ok(Followed::Fixed(how));
    }
    if row.state != "handed_off" {
        return Ok(Followed::Working);
    }
    let session = row.fixer_session.ok_or_else(|| missing("fixer session"))?;
    let since = row.handed_off_at.unwrap_or(row.updated_at);
    let Some(why) = ctx.sessions.ended(session, since).await? else {
        return Ok(Followed::Working);
    };
    if let Some(sha) = batch_fixer_commit(ctx, limits, row).await? {
        return Ok(Followed::Batched(sha));
    }
    let state = web
        .return_wright_followup(row.id, limits.max_attempts)
        .await?
        .ok_or_else(|| missing("hand-off to return"))?;
    Ok(Followed::Returned { state, why })
}

pub(crate) fn find_trailer_commit(stdout: &str, followup: i64) -> Option<String> {
    parse_log(stdout)
        .into_iter()
        .find(|commit| trailer_id(&commit.body, "Wright-Followup") == Some(followup))
        .map(|commit| commit.sha)
}

async fn batch_log<R, S>(
    ctx: &FollowupContext<R, S>,
    limits: &FollowupLimits,
    row: &FollowupRow,
    batch: &FixBatchRow,
) -> Result<String, FollowupError>
where
    R: CommandRunner,
{
    let machine = row.fixer_machine.as_deref().unwrap_or_default();
    let target = ctx
        .here
        .targets(&limits.fixers)
        .into_iter()
        .find(|target| target.machine.eq_ignore_ascii_case(machine))
        .ok_or_else(|| {
            FollowupError::Output(format!(
                "fixer machine {machine} cannot be reached from here to read its commits"
            ))
        })?;
    let args = ["-C", &batch.worktree, "log", LOG_FORMAT, &batch.branch];
    let cmd = match &target.place {
        Place::Local => Cmd::new(Program::Git, &args),
        Place::Remote(host) => Cmd::remote(host, Program::Git, &args),
    };
    Ok(checked(&ctx.runner, &cmd).await?)
}

async fn batch_fixer_commit<R, S>(
    ctx: &FollowupContext<R, S>,
    limits: &FollowupLimits,
    row: &FollowupRow,
) -> Result<Option<String>, FollowupError>
where
    R: CommandRunner,
{
    let web = ctx.persist.web();
    let Some(batch) = web.open_wright_fix_batch(None).await? else {
        return Ok(None);
    };
    let stdout = batch_log(ctx, limits, row, &batch).await?;
    let Some(sha) = find_trailer_commit(&stdout, row.id) else {
        return Ok(None);
    };
    let marked = web
        .mark_wright_followup_batched(row.id, batch.id, &sha)
        .await?;
    Ok(marked.then_some(sha))
}
