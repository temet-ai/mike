use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use super::runner::{Cmd, CommandRunner, Program, StepError, git, output};

pub(crate) const REMOTE: &str = "origin";
pub(crate) const MAIN: &str = "main";
pub(crate) const MAIN_TRACKING: &str = "refs/remotes/origin/main";
const MAIN_REFSPEC: &str = "+refs/heads/main:refs/remotes/origin/main";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepoLayout {
    pub(crate) root: PathBuf,
    pub(crate) hooks: PathBuf,
}

impl RepoLayout {
    pub(crate) fn worktrees(&self) -> PathBuf {
        self.root.join(".claude").join("worktrees")
    }

    pub(crate) fn hooks_config(&self) -> String {
        format!("core.hooksPath={}", self.hooks.display())
    }

    pub(crate) async fn discover<R: CommandRunner>(
        runner: &R,
        cwd: &Path,
    ) -> Result<Self, StepError> {
        let common = git(
            runner,
            cwd,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )
        .await?;
        let common = PathBuf::from(common);
        let root = common
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| StepError {
                step: "locate the repository root".to_owned(),
                code: None,
                detail: format!("{} has no parent directory", common.display()),
            })?;
        let configured = output(
            runner,
            &Cmd::new(Program::Git, &root, &["config", "--get", "core.hooksPath"]),
        )
        .await?;
        let hooks = match configured.stdout.trim() {
            "" => common.join("hooks"),
            path => root.join(path),
        };
        Ok(Self { root, hooks })
    }

    pub(crate) fn batch_dir(&self, day: NaiveDate) -> PathBuf {
        self.worktrees().join(format!("wright-fixes-{day}"))
    }
}

pub(crate) fn batch_branch(day: NaiveDate) -> String {
    format!("wright/fixes-{day}")
}

pub(crate) async fn fetch_main<R: CommandRunner>(
    runner: &R,
    cwd: &Path,
) -> Result<String, StepError> {
    git(runner, cwd, &["fetch", "--quiet", REMOTE, MAIN_REFSPEC]).await?;
    git(runner, cwd, &["rev-parse", MAIN_TRACKING]).await
}

pub(crate) async fn follow_main<R: CommandRunner>(
    runner: &R,
    repo: &RepoLayout,
    path: &Path,
) -> Result<String, StepError> {
    fetch_main(runner, &repo.root).await?;
    let cmd = Cmd::new(
        Program::Git,
        path,
        &["merge", "--ff-only", "--quiet", MAIN_TRACKING],
    );
    let _ = output(runner, &cmd).await;
    git(runner, path, &["rev-parse", "HEAD"]).await
}

async fn branch_exists<R: CommandRunner>(
    runner: &R,
    cwd: &Path,
    branch: &str,
) -> Result<bool, StepError> {
    let reference = format!("refs/heads/{branch}");
    let out = output(
        runner,
        &Cmd::new(
            Program::Git,
            cwd,
            &["rev-parse", "--verify", "--quiet", &reference],
        ),
    )
    .await?;
    Ok(out.ok())
}

pub(crate) fn is_repair_branch(first: &str, branch: &str) -> bool {
    branch == first
        || branch
            .strip_prefix(first)
            .and_then(|rest| rest.strip_prefix('-'))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

fn same_path(listed: &str, path: &Path) -> bool {
    let listed = Path::new(listed);
    listed == path
        || matches!(
            (listed.canonicalize(), path.canonicalize()),
            (Ok(a), Ok(b)) if a == b
        )
}

pub(crate) fn listed_as_ours(porcelain: &str, path: &Path, first_branch: &str) -> bool {
    porcelain.split("\n\n").any(|block| {
        let mut at = None;
        let mut branch = None;
        for line in block.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                at = Some(p);
            } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
                branch = Some(b);
            }
        }
        at.is_some_and(|at| same_path(at, path))
            && branch.is_some_and(|b| is_repair_branch(first_branch, b))
    })
}

pub(crate) async fn open_batch<R: CommandRunner>(
    runner: &R,
    repo: &RepoLayout,
    path: &Path,
    first_branch: &str,
    branch: &str,
) -> Result<String, String> {
    let step = |what: &'static str| move |e: StepError| format!("{what}: {e}");
    git(runner, &repo.root, &["worktree", "prune"])
        .await
        .map_err(step("could not prune worktrees"))?;
    let list = git(runner, &repo.root, &["worktree", "list", "--porcelain"])
        .await
        .map_err(step("could not list worktrees"))?;
    if !listed_as_ours(&list, path, first_branch) {
        create_batch(runner, repo, path, branch).await?;
    }
    git(runner, path, &["rev-parse", "HEAD"])
        .await
        .map_err(step("could not read the batch head"))
}

async fn create_batch<R: CommandRunner>(
    runner: &R,
    repo: &RepoLayout,
    path: &Path,
    branch: &str,
) -> Result<(), String> {
    if path.exists() {
        return Err(format!(
            "{} already exists and is not this batch's; it was left alone",
            path.display()
        ));
    }
    let base = fetch_main(runner, &repo.root)
        .await
        .map_err(|e| format!("could not read origin/main: {e}"))?;
    let exists = branch_exists(runner, &repo.root, branch)
        .await
        .map_err(|e| e.to_string())?;
    let shown = path.display().to_string();
    let args: Vec<&str> = if exists {
        vec!["worktree", "add", "--quiet", &shown, branch]
    } else {
        vec!["worktree", "add", "--quiet", "-b", branch, &shown, &base]
    };
    git(runner, &repo.root, &args)
        .await
        .map(|_| ())
        .map_err(|e| format!("could not create the worktree: {e}"))
}

pub(crate) async fn restore<R: CommandRunner>(
    runner: &R,
    worktree: &Path,
    to: &str,
) -> Result<(), StepError> {
    git(runner, worktree, &["reset", "--quiet", "--hard", to]).await?;
    git(runner, worktree, &["clean", "--quiet", "-fd"])
        .await
        .map(|_| ())
}

pub(crate) async fn restore_if_ours<R: CommandRunner>(
    runner: &R,
    repo: &RepoLayout,
    path: &Path,
    first_branch: &str,
    to: &str,
) -> Result<bool, StepError> {
    let list = git(runner, &repo.root, &["worktree", "list", "--porcelain"]).await?;
    if !listed_as_ours(&list, path, first_branch) {
        return Ok(false);
    }
    restore(runner, path, to).await.map(|()| true)
}

pub(crate) async fn close<R: CommandRunner>(
    runner: &R,
    repo: &RepoLayout,
    path: &Path,
    first_branch: &str,
) -> Result<bool, StepError> {
    let list = git(runner, &repo.root, &["worktree", "list", "--porcelain"]).await?;
    if !listed_as_ours(&list, path, first_branch) {
        return Ok(false);
    }
    let shown = path.display().to_string();
    git(
        runner,
        &repo.root,
        &["worktree", "remove", "--force", &shown],
    )
    .await?;
    Ok(true)
}

pub(crate) async fn remove_batch<R: CommandRunner>(
    runner: &R,
    repo: &RepoLayout,
    path: &Path,
    branches: &[String],
) -> Result<(), StepError> {
    let Some(first) = branches.first() else {
        return Ok(());
    };
    close(runner, repo, path, first).await?;
    for branch in branches {
        output(
            runner,
            &Cmd::new(
                Program::Git,
                &repo.root,
                &["branch", "--quiet", "-D", branch],
            ),
        )
        .await?;
    }
    Ok(())
}
