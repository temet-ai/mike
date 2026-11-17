use crate::store::persistence::web_repo::WebPersist as _;

use super::FollowupError;
use super::context::FollowupContext;
use super::runner::{Cmd, CommandRunner, Program, checked};
use crate::store::persistence::wright::{FollowupLimits, RecurringCause};

const PER_TICK: i64 = 5;

pub(crate) fn comment_body(cause: &RecurringCause) -> String {
    format!(
        "Wright saw this again since {since}: check `{check}`, {problems} open problem(s) sharing this cause, observed {observations} times in their open episodes. Newest example: `{resource}`.\n\nThis is another occurrence of the cause behind follow-up {followup}, not a new problem; Wright will not investigate it again while this issue is open.\n\n<!-- wright-cause:{cause_id} -->",
        since = cause.since.to_rfc3339(),
        check = cause.check_name,
        problems = cause.problems,
        observations = cause.observations,
        resource = cause.resource,
        followup = cause.followup_id,
        cause_id = cause.cause_id,
    )
}

async fn comment_one<R, S>(
    ctx: &FollowupContext<R, S>,
    repo: &str,
    cause: &RecurringCause,
) -> Result<(), FollowupError>
where
    R: CommandRunner,
{
    let number = cause.issue_number.to_string();
    let cmd = Cmd::new(
        Program::Gh,
        &["issue", "comment", &number, "-R", repo, "--body-file", "-"],
    )
    .with_stdin(comment_body(cause));
    checked(&ctx.runner, &cmd).await?;
    ctx.persist
        .web()
        .mark_wright_cause_commented(cause.cause_id)
        .await?;
    Ok(())
}

pub(crate) async fn comment_recurrences<R, S>(
    ctx: &FollowupContext<R, S>,
    limits: &FollowupLimits,
) -> Result<Vec<(i64, String)>, FollowupError>
where
    R: CommandRunner,
{
    let mut failed = Vec::new();
    for cause in ctx.persist.web().recurring_wright_causes(PER_TICK).await? {
        if let Err(e) = comment_one(ctx, &limits.repo, &cause).await {
            failed.push((cause.cause_id, e.to_string()));
        }
    }
    Ok(failed)
}
