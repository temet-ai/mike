use std::path::Path;

use super::prepare::as_wright;
use super::runner::{Cmd, CommandRunner, Program, StepError, checked, git, output};
use super::worktree::{REMOTE, RepoLayout};
use crate::store::persistence::wright::BatchFix;

const MAX_DROPS: usize = 64;
pub(crate) const LOG_FORMAT: &str = "--format=%x1e%H%x1f%B";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Commit {
    pub(crate) sha: String,
    pub(crate) body: String,
}

pub(crate) fn trailer_id(body: &str, key: &str) -> Option<i64> {
    body.lines().find_map(|line| {
        line.strip_prefix(key)?
            .strip_prefix(':')?
            .trim()
            .parse()
            .ok()
    })
}

pub(super) fn fix_of(body: &str) -> Option<(bool, i64)> {
    if let Some(id) = trailer_id(body, "Wright-Repair") {
        return Some((true, id));
    }
    trailer_id(body, "Wright-Followup").map(|id| (false, id))
}

pub(crate) fn parse_log(raw: &str) -> Vec<Commit> {
    raw.split('\u{1e}')
        .filter(|record| !record.trim().is_empty())
        .filter_map(|record| {
            let (sha, body) = record.split_once('\u{1f}')?;
            Some(Commit {
                sha: sha.trim().to_owned(),
                body: body.trim_end().to_owned(),
            })
        })
        .collect()
}

pub(super) fn locate<'a>(commits: &'a [Commit], fix: &BatchFix) -> Option<&'a Commit> {
    commits
        .iter()
        .find(|commit| fix_of(&commit.body) == Some((fix.is_repair(), fix.id)))
}

pub(super) async fn commits_between<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
    base: &str,
) -> Result<Vec<Commit>, StepError> {
    let range = format!("{base}..HEAD");
    let raw = git(runner, worktree, &["log", "--reverse", LOG_FORMAT, &range]).await?;
    Ok(parse_log(&raw))
}

pub(super) async fn changed_paths<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
    sha: &str,
) -> Result<Vec<String>, StepError> {
    let raw = git(
        runner,
        worktree,
        &["show", "--name-only", "--no-renames", "--format=", sha],
    )
    .await?;
    Ok(raw.lines().map(str::to_owned).collect())
}

pub(super) async fn abort_stale<R: CommandRunner>(runner: &R, worktree: &Path) {
    for verb in ["rebase", "revert", "cherry-pick", "merge"] {
        let cmd = Cmd::new(Program::Git, worktree, &[verb, "--abort"]);
        let _ = output(runner, &cmd).await;
    }
}

async fn conflicting_commit<R: CommandRunner>(runner: &R, worktree: &Path) -> Option<Commit> {
    let raw = git(runner, worktree, &["log", "-1", LOG_FORMAT, "REBASE_HEAD"])
        .await
        .ok()?;
    parse_log(&raw).into_iter().next()
}

pub(super) async fn rebase_dropping<R: CommandRunner>(
    runner: &R,
    repo: &RepoLayout,
    worktree: &Path,
    main: &str,
) -> Result<Vec<Commit>, String> {
    let hooks = repo.hooks_config();
    let mut dropped: Vec<Commit> = Vec::new();
    for _ in 0..MAX_DROPS {
        let verb: &[&str] = if dropped.is_empty() {
            &["rebase", main]
        } else {
            &["rebase", "--skip"]
        };
        let args = as_wright(&hooks, verb);
        let out = output(runner, &Cmd::new(Program::Git, worktree, &args))
            .await
            .map_err(|e| e.to_string())?;
        if out.ok() {
            return Ok(dropped);
        }
        match conflicting_commit(runner, worktree).await {
            Some(commit) if fix_of(&commit.body).is_some() => dropped.push(commit),
            _ => {
                abort_stale(runner, worktree).await;
                return Err(format!(
                    "the rebase onto main {main} stopped on something that is not a fix: {}",
                    out.combined()
                ));
            }
        }
    }
    abort_stale(runner, worktree).await;
    Err(format!(
        "more than {MAX_DROPS} fixes conflicted with main {main}"
    ))
}

pub(super) async fn revert<R: CommandRunner>(
    runner: &R,
    repo: &RepoLayout,
    worktree: &Path,
    sha: &str,
) -> Result<(), String> {
    let hooks = repo.hooks_config();
    let args = as_wright(&hooks, &["revert", "--no-edit", sha]);
    if let Err(e) = checked(runner, &Cmd::new(Program::Git, worktree, &args)).await {
        abort_stale(runner, worktree).await;
        return Err(e.to_string());
    }
    Ok(())
}

pub(super) async fn commit_round<R: CommandRunner>(
    runner: &R,
    repo: &RepoLayout,
    worktree: &Path,
    message: &str,
) -> Result<Option<String>, StepError> {
    git(runner, worktree, &["add", "--all"]).await?;
    let staged = git(runner, worktree, &["diff", "--cached", "--name-only"]).await?;
    if staged.is_empty() {
        return Ok(None);
    }
    let hooks = repo.hooks_config();
    let args = as_wright(&hooks, &["commit", "--quiet", "-m", message]);
    checked(runner, &Cmd::new(Program::Git, worktree, &args)).await?;
    git(runner, worktree, &["rev-parse", "HEAD"])
        .await
        .map(Some)
}

pub(super) async fn remote_sha<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
    branch: &str,
) -> Result<Option<String>, StepError> {
    let raw = git(
        runner,
        worktree,
        &["ls-remote", REMOTE, &format!("refs/heads/{branch}")],
    )
    .await?;
    Ok(raw.split_whitespace().next().map(str::to_owned))
}

pub(super) async fn switch_new<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
    branch: &str,
) -> Result<(), StepError> {
    git(runner, worktree, &["switch", "--quiet", "-c", branch])
        .await
        .map(|_| ())
}

pub(super) async fn head<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
) -> Result<String, StepError> {
    git(runner, worktree, &["rev-parse", "HEAD"]).await
}

#[cfg(test)]
mod tests {
    use super::{Commit, fix_of, parse_log, trailer_id};

    #[test]
    fn trailers_identify_a_repair_or_a_followup_commit() {
        let repair = "Wright repair 4\n\nWright-Repair: 4\nWright-Finding: 9";
        let followup = "Fix the thing\n\nFixes #12\nWright-Followup: 7";
        assert_eq!(fix_of(repair), Some((true, 4)));
        assert_eq!(fix_of(followup), Some((false, 7)));
        assert_eq!(fix_of("a plain commit"), None);
        assert_eq!(trailer_id(repair, "Wright-Finding"), Some(9));
    }

    #[test]
    fn log_records_split_on_their_separators() {
        let raw = "\u{1e}aaa\u{1f}first\n\nWright-Repair: 1\n\u{1e}bbb\u{1f}second";
        assert_eq!(
            parse_log(raw),
            vec![
                Commit {
                    sha: "aaa".to_owned(),
                    body: "first\n\nWright-Repair: 1".to_owned()
                },
                Commit {
                    sha: "bbb".to_owned(),
                    body: "second".to_owned()
                },
            ]
        );
    }
}
