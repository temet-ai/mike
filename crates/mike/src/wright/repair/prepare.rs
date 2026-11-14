use std::path::Path;

use super::policy::{self, ChangedFile};
use super::runner::{Cmd, CommandRunner, Program, StepError, checked, git};
use super::worktree::RepoLayout;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Prepared {
    Empty,
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

pub(crate) async fn commit_patch<R: CommandRunner>(
    runner: &R,
    repo: &RepoLayout,
    worktree: &Path,
    base: &str,
    message: &str,
) -> Result<Prepared, StepError> {
    git(runner, worktree, &["add", "--all"]).await?;
    let staged = git(runner, worktree, &["diff", "--cached", "--name-only"]).await?;
    if staged.is_empty() {
        return Ok(Prepared::Empty);
    }
    let hooks = repo.hooks_config();
    let args = as_wright(&hooks, &["commit", "--quiet", "-m", message]);
    checked(runner, &Cmd::new(Program::Git, worktree, &args)).await?;
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
