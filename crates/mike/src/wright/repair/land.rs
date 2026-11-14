use crate::store::persistence::web_repo::WebPersist as _;
use std::path::Path;
use std::time::Duration;

use super::prepare::{as_wright, changed_files, refusal};
use super::runner::{Cmd, CommandRunner, Program, StepError, checked, git, output};
use super::states;
use super::worktree::{MAIN, REMOTE, RepoLayout, fetch_main};
use crate::store::persistence::wright::RepairUpdate;
use crate::store::persistence::{PersistError, PersistHandle};

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
    Persist(PersistError),
    Failed(String),
    Unsettled(String),
}

impl From<PersistError> for LandError {
    fn from(e: PersistError) -> Self {
        LandError::Persist(e)
    }
}

enum Merge {
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

pub(crate) struct Landing<'a, R> {
    pub(crate) persist: &'a PersistHandle,
    pub(crate) runner: &'a R,
    pub(crate) repo: &'a RepoLayout,
    pub(crate) worktree: &'a Path,
    pub(crate) repair_id: i64,
    pub(crate) first_branch: &'a str,
    pub(crate) title: String,
    pub(crate) body: String,
}

impl<R: CommandRunner> Landing<'_, R> {
    pub(crate) async fn land(&self, first: Candidate) -> Result<Landed, LandError> {
        let mut cand = first;
        for attempt in 1..=MAX_GATES {
            self.record(RepairUpdate {
                to: states::GATING,
                from: &[states::PREPARED],
                branch: Some(cand.branch.clone()),
                ..self.update()
            })
            .await?;
            self.run_gate(&cand).await?;
            let pr = self.open_pr(&cand).await?;
            let main_now = match self.merge_unless_moved(&cand, &pr).await? {
                Merge::Landed(landed) => return Ok(landed),
                Merge::MainMoved(main_now) => main_now,
            };
            self.close_pr(&pr, &main_now).await;
            if attempt == MAX_GATES {
                break;
            }
            cand = self.rebase(&cand, &main_now, attempt + 1).await?;
        }
        Err(LandError::Failed(format!(
            "main moved during each of {MAX_GATES} gates; the patch was not landed"
        )))
    }

    fn update(&self) -> RepairUpdate {
        RepairUpdate {
            id: self.repair_id,
            to: states::GATING,
            from: &[states::GATING],
            ..RepairUpdate::default()
        }
    }

    async fn record(&self, update: RepairUpdate) -> Result<(), LandError> {
        if self.persist.web().advance_wright_repair(&update).await? {
            return Ok(());
        }
        Err(LandError::Failed(format!(
            "repair {} left the state the executor expected before it could become {}",
            self.repair_id, update.to
        )))
    }

    async fn run_gate(&self, cand: &Candidate) -> Result<(), LandError> {
        let result = gate(self.runner, self.repo, self.worktree, cand)
            .await
            .map_err(failed("the gate could not run"))?;
        let exit = result.exit.unwrap_or(-1);
        let (gate_output, bytes) = kept_output(result.output);
        self.record(RepairUpdate {
            gate_exit: Some(exit),
            gate_output,
            gate_output_bytes: Some(bytes),
            ..self.update()
        })
        .await?;
        if result.passed {
            return Ok(());
        }
        Err(LandError::Failed(format!(
            "the push hook did not pass commit {} (exit {exit})",
            cand.sha
        )))
    }

    async fn open_pr(&self, cand: &Candidate) -> Result<String, LandError> {
        let args: [&str; 10] = [
            "pr",
            "create",
            "--base",
            MAIN,
            "--head",
            &cand.branch,
            "--title",
            &self.title,
            "--body",
            &self.body,
        ];
        let out = checked(self.runner, &Cmd::new(Program::Gh, self.worktree, &args))
            .await
            .map_err(failed("the pull request could not be opened"))?;
        let url = out.lines().last().unwrap_or_default().trim().to_owned();
        self.record(RepairUpdate {
            pr_url: Some(url.clone()),
            ..self.update()
        })
        .await?;
        Ok(url)
    }

    async fn main_now(&self) -> Result<String, LandError> {
        fetch_main(self.runner, self.worktree)
            .await
            .map_err(failed("origin/main could not be read"))
    }

    async fn merge_unless_moved(&self, cand: &Candidate, pr: &str) -> Result<Merge, LandError> {
        let main_now = self.main_now().await?;
        if main_now != cand.base {
            return Ok(Merge::MainMoved(main_now));
        }
        let args: [&str; 6] = [
            "pr",
            "merge",
            pr,
            "--squash",
            "--match-head-commit",
            &cand.sha,
        ];
        if let Err(refused) =
            checked(self.runner, &Cmd::new(Program::Gh, self.worktree, &args)).await
        {
            let main_now = self.main_now().await?;
            if main_now != cand.base {
                return Ok(Merge::MainMoved(main_now));
            }
            return Err(LandError::Failed(format!(
                "the merge was refused: {refused}"
            )));
        }
        let merge_sha = merged_commit(self.runner, self.worktree, pr)
            .await
            .map_err(|e| {
                LandError::Unsettled(format!(
                    "merged, but the merge commit could not be read: {e}"
                ))
            })?
            .ok_or_else(|| {
                LandError::Unsettled("merged, but GitHub reports no merge commit yet".to_owned())
            })?;
        let note = self.landed_note(cand, &merge_sha).await;
        Ok(Merge::Landed(Landed { merge_sha, note }))
    }

    async fn rev(&self, spec: &str) -> Result<String, StepError> {
        git(self.runner, self.worktree, &["rev-parse", spec]).await
    }

    async fn landed_note(&self, cand: &Candidate, merge_sha: &str) -> Option<String> {
        let _ = self.main_now().await;
        let landed_tree = self.rev(&format!("{merge_sha}^{{tree}}")).await;
        let gated_tree = self.rev(&format!("{}^{{tree}}", cand.sha)).await;
        let parent = self.rev(&format!("{merge_sha}^1")).await;
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

    async fn close_pr(&self, pr: &str, main_now: &str) {
        let comment = format!(
            "Main moved to {main_now} after this gate; Wright gates the patch again on a new branch."
        );
        let args: [&str; 5] = ["pr", "close", pr, "--comment", &comment];
        if let Err(e) = checked(self.runner, &Cmd::new(Program::Gh, self.worktree, &args)).await {
            tracing::warn!(error = %e, pr, "wright repair: closing a superseded pull request failed");
        }
    }

    async fn rebase(
        &self,
        cand: &Candidate,
        main_now: &str,
        n: i32,
    ) -> Result<Candidate, LandError> {
        let branch = format!("{}-{n}", self.first_branch);
        git(
            self.runner,
            self.worktree,
            &["switch", "--quiet", "-c", &branch, main_now],
        )
        .await
        .map_err(failed("the new branch could not be made"))?;
        let hooks = self.repo.hooks_config();
        let args = as_wright(&hooks, &["cherry-pick", &cand.sha]);
        if let Err(e) = checked(self.runner, &Cmd::new(Program::Git, self.worktree, &args)).await {
            let _ = output(
                self.runner,
                &Cmd::new(Program::Git, self.worktree, &["cherry-pick", "--abort"]),
            )
            .await;
            return Err(LandError::Failed(format!(
                "the patch does not apply on main {main_now}: {e}"
            )));
        }
        let sha = git(self.runner, self.worktree, &["rev-parse", "HEAD"])
            .await
            .map_err(failed("the rebased commit could not be read"))?;
        self.prepared(Candidate {
            branch,
            base: main_now.to_owned(),
            sha,
        })
        .await
    }

    async fn prepared(&self, cand: Candidate) -> Result<Candidate, LandError> {
        let files = changed_files(self.runner, self.worktree, &cand.base, &cand.sha)
            .await
            .map_err(failed("the rebased patch could not be read"))?;
        if let Some(reason) = refusal(self.runner, self.worktree, &cand.base, &cand.sha, &files)
            .await
            .map_err(failed("the rebased patch could not be checked"))?
        {
            return Err(LandError::Failed(reason));
        }
        self.record(RepairUpdate {
            to: states::PREPARED,
            branch: Some(cand.branch.clone()),
            base_sha: Some(cand.base.clone()),
            commit_sha: Some(cand.sha.clone()),
            patch: Some(super::policy::patch_summary(&files)),
            ..self.update()
        })
        .await?;
        Ok(cand)
    }
}
