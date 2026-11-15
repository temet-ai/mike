use crate::store::persistence::web_repo::WebPersist as _;
use serde::Deserialize;

use super::FollowupError;
use super::body::{issue_body, issue_title, marker};
use super::context::FollowupContext;
use super::runner::{Cmd, CommandRunner, Program, checked};
use crate::store::persistence::wright::{FollowupLimits, FollowupRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Filing {
    Adopted(i32),
    Created(i32),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct FileReport {
    pub(crate) capped: bool,
    pub(crate) filed: Vec<(i64, Filing)>,
    pub(crate) failed: Vec<(i64, String)>,
}

#[derive(Debug, Deserialize)]
struct Listed {
    number: i32,
    url: String,
    #[serde(default)]
    body: String,
}

pub(crate) async fn file<R, S>(
    ctx: &FollowupContext<R, S>,
    limits: &FollowupLimits,
) -> Result<FileReport, FollowupError>
where
    R: CommandRunner,
{
    let web = ctx.persist.web();
    let room = i64::from(limits.daily_filed) - web.wright_followups_filed_today().await?;
    let mut report = FileReport::default();
    if room <= 0 {
        report.capped = true;
        return Ok(report);
    }
    for row in web.wright_followups_to_file(room).await? {
        match file_one(ctx, &limits.repo, &row).await {
            Ok(filing) => report.filed.push((row.id, filing)),
            Err(e) => report.failed.push((row.id, e.to_string())),
        }
    }
    Ok(report)
}

async fn file_one<R, S>(
    ctx: &FollowupContext<R, S>,
    repo: &str,
    row: &FollowupRow,
) -> Result<Filing, FollowupError>
where
    R: CommandRunner,
{
    let (filing, url) = match existing(&ctx.runner, repo, row.id).await? {
        Some(found) => (Filing::Adopted(found.number), found.url),
        None => {
            let (number, url) = create(&ctx.runner, repo, row).await?;
            (Filing::Created(number), url)
        }
    };
    let number = match filing {
        Filing::Adopted(number) | Filing::Created(number) => number,
    };
    if !ctx
        .persist
        .web()
        .mark_wright_followup_filed(row.id, number, &url)
        .await?
    {
        return Err(FollowupError::Output(format!(
            "follow-up {} was no longer open when issue #{number} was recorded",
            row.id
        )));
    }
    Ok(filing)
}

async fn existing<R: CommandRunner>(
    runner: &R,
    repo: &str,
    id: i64,
) -> Result<Option<Listed>, FollowupError> {
    let search = format!("--search=wright-followup:{id} in:body");
    let cmd = Cmd::new(
        Program::Gh,
        &[
            "issue",
            "list",
            "-R",
            repo,
            "--label",
            "wright",
            "--state",
            "all",
            &search,
            "--json",
            "number,url,body",
        ],
    );
    let out = checked(runner, &cmd).await?;
    let listed: Vec<Listed> = serde_json::from_str(&out).map_err(|e| {
        FollowupError::Output(format!("`{}` printed {out:?}, not JSON: {e}", cmd.line()))
    })?;
    let wanted = marker(id);
    Ok(listed
        .into_iter()
        .find(|issue| issue.body.contains(&wanted)))
}

pub(crate) fn parse_created(stdout: &str) -> Option<(i32, String)> {
    let url = stdout
        .lines()
        .map(str::trim)
        .rev()
        .find(|line| line.starts_with("https://") && line.contains("/issues/"))?;
    let number = url.rsplit('/').next()?.parse().ok()?;
    Some((number, url.to_owned()))
}

async fn create<R: CommandRunner>(
    runner: &R,
    repo: &str,
    row: &FollowupRow,
) -> Result<(i32, String), FollowupError> {
    let title = format!("--title={}", issue_title(row));
    let cmd = Cmd::new(
        Program::Gh,
        &[
            "issue",
            "create",
            "-R",
            repo,
            "--label",
            "wright",
            &title,
            "--body-file",
            "-",
        ],
    )
    .with_stdin(issue_body(row));
    let out = checked(runner, &cmd).await?;
    parse_created(&out).ok_or_else(|| {
        FollowupError::Output(format!(
            "`gh issue create` for follow-up {} printed {out:?}, with no issue URL in it",
            row.id
        ))
    })
}
