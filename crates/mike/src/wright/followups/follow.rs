use crate::store::persistence::web_repo::WebPersist as _;

use serde::Deserialize;

use super::FollowupError;
use super::context::FollowupContext;
use super::runner::{Cmd, CommandRunner, Program, checked};
use super::sessions::FixerSessions;
use crate::store::persistence::wright::{FollowupLimits, FollowupRow};

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
    for row in ctx.persist.web().live_wright_handoffs().await? {
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
    let session = row.fixer_session.ok_or_else(|| missing("fixer session"))?;
    let since = row.handed_off_at.unwrap_or(row.updated_at);
    let Some(why) = ctx.sessions.ended(session, since).await? else {
        return Ok(Followed::Working);
    };
    let state = web
        .return_wright_followup(row.id, limits.max_attempts)
        .await?
        .ok_or_else(|| missing("hand-off to return"))?;
    Ok(Followed::Returned { state, why })
}
