use std::path::Path;
use std::time::Duration;

use super::runner::{Cmd, CommandRunner, Program, StepError, checked, git, output};
use super::worktree::{MAIN, REMOTE, RepoLayout, fetch_main};

const GATE_TIMEOUT: Duration = Duration::from_secs(90 * 60);
pub(crate) const MAX_GATES: i32 = 3;
pub(crate) const GATE_OUTPUT_MAX: usize = 512 * 1024;
const MERGED_COMMIT: &str = r#"if .state == "MERGED" then .mergeCommit.oid else "" end"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) branch: String,
    pub(crate) base: String,
    pub(crate) sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Landed {
    pub(crate) merge_sha: String,
    pub(crate) note: Option<String>,
}

#[derive(Debug)]
pub(crate) enum LandError {
    Failed(String),
    Unsettled(String),
}

pub(crate) enum Merge {
    Landed(Landed),
    MainMoved(String),
}

pub(crate) struct GateResult {
    pub(crate) exit: Option<i32>,
    pub(crate) output: String,
    pub(crate) passed: bool,
}

pub(crate) fn kept_output(output: String) -> (Option<String>, i32) {
    let bytes = i32::try_from(output.len()).unwrap_or(i32::MAX);
    ((output.len() <= GATE_OUTPUT_MAX).then_some(output), bytes)
}

fn failed(context: &str) -> impl Fn(StepError) -> LandError + '_ {
    move |e| LandError::Failed(format!("{context}: {e}"))
}

pub(crate) async fn gate<R: CommandRunner>(
    runner: &R,
    repo: &RepoLayout,
    worktree: &Path,
    cand: &Candidate,
) -> Result<GateResult, StepError> {
    let local = git(
        runner,
        worktree,
        &["rev-parse", &format!("refs/heads/{}", cand.branch)],
    )
    .await?;
    if local != cand.sha {
        return Err(StepError {
            step: "gate".to_owned(),
            code: None,
            detail: format!(
                "branch {} is at {local}, not the prepared commit {}",
                cand.branch, cand.sha
            ),
        });
    }
    let hooks = repo.hooks_config();
    let refspec = format!("refs/heads/{0}:refs/heads/{0}", cand.branch);
    let push = Cmd::new(
        Program::Git,
        worktree,
        &["-c", &hooks, "push", REMOTE, &refspec],
    )
    .with_timeout(GATE_TIMEOUT);
    let out = output(runner, &push).await?;
    let remote = git(
        runner,
        worktree,
        &["ls-remote", REMOTE, &format!("refs/heads/{}", cand.branch)],
    )
    .await?;
    let remote = remote.split_whitespace().next().unwrap_or_default();
    Ok(GateResult {
        exit: out.code,
        passed: out.ok() && remote == cand.sha,
        output: out.combined(),
    })
}

pub(crate) async fn merged_commit<R: CommandRunner>(
    runner: &R,
    cwd: &Path,
    url: &str,
) -> Result<Option<String>, StepError> {
    let oid = checked(
        runner,
        &Cmd::new(
            Program::Gh,
            cwd,
            &[
                "pr",
                "view",
                url,
                "--json",
                "state,mergeCommit",
                "--jq",
                MERGED_COMMIT,
            ],
        ),
    )
    .await?;
    Ok(Some(oid).filter(|oid| !oid.is_empty() && oid != "null"))
}

pub(crate) async fn open_pr<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
    branch: &str,
    title: &str,
    body: &str,
) -> Result<String, LandError> {
    let args: [&str; 10] = [
        "pr", "create", "--base", MAIN, "--head", branch, "--title", title, "--body", body,
    ];
    let out = checked(runner, &Cmd::new(Program::Gh, worktree, &args))
        .await
        .map_err(failed("the pull request could not be opened"))?;
    Ok(out.lines().last().unwrap_or_default().trim().to_owned())
}

pub(crate) async fn main_now<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
) -> Result<String, LandError> {
    fetch_main(runner, worktree)
        .await
        .map_err(failed("origin/main could not be read"))
}

pub(crate) async fn merge_unless_moved<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
    cand: &Candidate,
    pr: &str,
) -> Result<Merge, LandError> {
    let now = main_now(runner, worktree).await?;
    if now != cand.base {
        return Ok(Merge::MainMoved(now));
    }
    let args: [&str; 6] = [
        "pr",
        "merge",
        pr,
        "--squash",
        "--match-head-commit",
        &cand.sha,
    ];
    if let Err(refused) = checked(runner, &Cmd::new(Program::Gh, worktree, &args)).await {
        let now = main_now(runner, worktree).await?;
        if now != cand.base {
            return Ok(Merge::MainMoved(now));
        }
        return Err(LandError::Failed(format!(
            "the merge was refused: {refused}"
        )));
    }
    let merge_sha = merged_commit(runner, worktree, pr)
        .await
        .map_err(|e| {
            LandError::Unsettled(format!(
                "merged, but the merge commit could not be read: {e}"
            ))
        })?
        .ok_or_else(|| {
            LandError::Unsettled("merged, but GitHub reports no merge commit yet".to_owned())
        })?;
    let note = landed_note(runner, worktree, cand, &merge_sha).await;
    Ok(Merge::Landed(Landed { merge_sha, note }))
}

async fn landed_note<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
    cand: &Candidate,
    merge_sha: &str,
) -> Option<String> {
    let _ = main_now(runner, worktree).await;
    let rev = |spec: String| async move { git(runner, worktree, &["rev-parse", &spec]).await };
    let landed_tree = rev(format!("{merge_sha}^{{tree}}")).await;
    let gated_tree = rev(format!("{}^{{tree}}", cand.sha)).await;
    let parent = rev(format!("{merge_sha}^1")).await;
    match (landed_tree, gated_tree, parent) {
        (Ok(landed), Ok(gated), Ok(parent)) if landed == gated && parent == cand.base => None,
        (Ok(_), Ok(_), Ok(parent)) => Some(format!(
            "main moved during the merge: {merge_sha} on {parent} is not the gated tree of {} on {}",
            cand.sha, cand.base
        )),
        _ => Some(format!(
            "the landed commit {merge_sha} could not be compared with the gated commit {}",
            cand.sha
        )),
    }
}

pub(crate) async fn close_pr<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
    pr: &str,
    comment: &str,
) {
    let args: [&str; 5] = ["pr", "close", pr, "--comment", comment];
    if let Err(e) = checked(runner, &Cmd::new(Program::Gh, worktree, &args)).await {
        tracing::warn!(error = %e, pr, "wright repair: closing a superseded pull request failed");
    }
}
