use std::path::Path;

use super::feedback::Refused;
use super::perf::{Evidence, PERF_DIR, Perf, clear, evidence, with_block};
use super::policy::{self, ChangedFile};
use super::runner::{Cmd, CommandRunner, Program, StepError, checked, git};
use super::worktree::RepoLayout;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Prepared {
    Empty,
    Refused(Refused),
    Barred(String),
    Commit {
        sha: String,
        files: Vec<ChangedFile>,
    },
}

pub(crate) fn as_wright<'a>(hooks: &'a str, rest: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec![
        "-c",
        "user.name=Wright",
        "-c",
        "user.email=wright@mike.invalid",
        "-c",
        "commit.gpgsign=false",
        "-c",
        hooks,
    ];
    args.extend_from_slice(rest);
    args
}

pub(crate) async fn stage_all<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
) -> Result<bool, StepError> {
    let skip = format!(":(exclude){PERF_DIR}");
    git(runner, worktree, &["add", "--all", "--", ".", &skip]).await?;
    let staged = git(runner, worktree, &["diff", "--cached", "--name-only"]).await?;
    Ok(!staged.is_empty())
}

async fn staged_barred<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
) -> Result<Option<String>, StepError> {
    let numstat = git(
        runner,
        worktree,
        &["diff", "--cached", "--numstat", "-z", "--no-renames"],
    )
    .await?;
    let migrations = git(
        runner,
        worktree,
        &[
            "diff",
            "--cached",
            "--no-renames",
            "-U0",
            "--",
            ":(glob)**/migrations/**",
        ],
    )
    .await?;
    Ok(policy::check(&policy::parse_numstat(&numstat), &migrations).err())
}

pub(crate) async fn commit_patch<R: CommandRunner>(
    runner: &R,
    repo: &RepoLayout,
    worktree: &Path,
    base: &str,
    message: &str,
    perf: &Perf,
) -> Result<Prepared, StepError> {
    if !stage_all(runner, worktree).await? {
        return Ok(Prepared::Empty);
    }
    if let Some(reason) = staged_barred(runner, worktree).await? {
        return Ok(Prepared::Barred(reason));
    }
    let block = match evidence(runner, worktree, perf).await? {
        Evidence::Block(block) => block,
        Evidence::Refused(refused) => return Ok(Prepared::Refused(refused)),
    };
    let message = with_block(message, &block);
    let hooks = repo.hooks_config();
    let args = as_wright(&hooks, &["commit", "--quiet", "-m", &message]);
    checked(runner, &Cmd::new(Program::Git, worktree, &args)).await?;
    clear(worktree).await;
    let sha = git(runner, worktree, &["rev-parse", "HEAD"]).await?;
    let files = changed_files(runner, worktree, base, &sha).await?;
    Ok(Prepared::Commit { sha, files })
}

pub(crate) async fn changed_files<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
    base: &str,
    sha: &str,
) -> Result<Vec<ChangedFile>, StepError> {
    let raw = git(
        runner,
        worktree,
        &["diff", "--numstat", "-z", "--no-renames", base, sha],
    )
    .await?;
    Ok(policy::parse_numstat(&raw))
}

pub(crate) async fn refusal<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
    base: &str,
    sha: &str,
    files: &[ChangedFile],
) -> Result<Option<String>, StepError> {
    let migrations = git(
        runner,
        worktree,
        &[
            "diff",
            "--no-renames",
            "-U0",
            base,
            sha,
            "--",
            ":(glob)**/migrations/**",
        ],
    )
    .await?;
    Ok(policy::check(files, &migrations).err())
}
