use crate::store::persistence::web_repo::WebPersist as _;
use std::path::PathBuf;

use super::batch_git::{self, Commit, locate};
use super::batch_mail::tell_operator;
use super::land::{
    self, Candidate, LandError, Landed, MAX_GATES, Merge, close_pr, gate, kept_output,
    merge_unless_moved, merged_commit, open_pr,
};
use super::port::RepairEngineer;
use super::runner::CommandRunner;
use super::worktree;
use super::{RepairContext, batch_states, states};
use crate::store::persistence::PersistError;
use crate::store::persistence::wright::{BatchFix, BatchUpdate, FixBatchRow, RepairUpdate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BatchEnd {
    Landed,
    Held,
    Interrupted,
}

pub(super) enum Gated {
    Passed(Candidate),
    Refused(Candidate, String),
    Stop(BatchEnd),
}

pub(super) enum Cycle {
    Merged(Landed),
    MainMoved,
    Stop(BatchEnd),
}

enum Rebuilt {
    Ready(Candidate),
    Stop(BatchEnd),
}

fn log_landed(batch: i64, merge: &str, repairs: &Option<Vec<i64>>) {
    tracing::info!(batch, %merge, repairs = ?repairs, "wright repair: the day's batch landed");
}

fn log_remove_failed(batch: i64, error: &str) {
    tracing::warn!(%error, batch, "wright repair: removing the landed batch's worktree failed");
}

pub(super) struct Lost {
    pub(super) fix: BatchFix,
    pub(super) reason: String,
}

pub(super) struct Lander<'a, R, E, B> {
    pub(super) ctx: &'a RepairContext<R, E, B>,
    pub(super) row: FixBatchRow,
    pub(super) path: PathBuf,
    pub(super) branches: Vec<String>,
}

pub(super) async fn land<R, E, B>(
    ctx: &RepairContext<R, E, B>,
    row: FixBatchRow,
) -> Result<BatchEnd, PersistError>
where
    R: CommandRunner,
    E: RepairEngineer,
{
    let mut lander = Lander::new(ctx, row);
    lander.run().await
}

pub(super) fn split_fixes(
    fixes: Vec<BatchFix>,
    commits: &[Commit],
    dropped: &[Commit],
    main: &str,
) -> (Vec<(BatchFix, String)>, Vec<Lost>) {
    let mut kept = Vec::new();
    let mut lost = Vec::new();
    for fix in fixes {
        if let Some(commit) = locate(commits, &fix) {
            kept.push((fix, commit.sha.clone()));
            continue;
        }
        let reason = if locate(dropped, &fix).is_some() {
            format!("it conflicts with main {main}, so the rebase dropped it")
        } else {
            format!("no commit on the branch carries its trailer after the rebase onto main {main}")
        };
        lost.push(Lost { fix, reason });
    }
    (kept, lost)
}

fn pr_body(row: &FixBatchRow, fixes: &[BatchFix]) -> String {
    let mut body = format!(
        "Wright's fixes for {}, gated once by the push hook on this branch.\n\n",
        row.day
    );
    for fix in fixes {
        let commit = fix.commit_sha.as_deref().unwrap_or("unknown");
        let part = fix.owning_part.as_deref().unwrap_or("no owning part");
        let finding = fix
            .finding_id
            .map_or_else(String::new, |id| format!("finding {id}; "));
        body.push_str(&format!(
            "- {}; {finding}owning part `{part}`; commit {commit}\n",
            fix.label
        ));
        if let Some(number) = fix.issue_number {
            body.push_str(&format!("  Fixes #{number}\n"));
        }
    }
    body
}

impl<'a, R, E, B> Lander<'a, R, E, B>
where
    R: CommandRunner,
    E: RepairEngineer,
{
    fn new(ctx: &'a RepairContext<R, E, B>, row: FixBatchRow) -> Self {
        let path = PathBuf::from(&row.worktree);
        let mut branches = row.branches.clone();
        if branches.is_empty() {
            branches.push(row.branch.clone());
        }
        Self {
            ctx,
            row,
            path,
            branches,
        }
    }

    fn base_branch(&self) -> &str {
        self.branches
            .first()
            .map_or(&self.row.branch, String::as_str)
    }

    pub(super) fn current_branch(&self) -> &str {
        self.branches
            .last()
            .map_or(&self.row.branch, String::as_str)
    }

    async fn run(&mut self) -> Result<BatchEnd, PersistError> {
        if let Some(end) = self.settle_open_pr().await? {
            return Ok(end);
        }
        if let Err(reason) = self.prepare().await {
            return self.stalled(&reason).await;
        }
        for _ in 1..=MAX_GATES {
            let main = match land::main_now(&self.ctx.runner, &self.path).await {
                Ok(main) => main,
                Err(LandError::Failed(reason) | LandError::Unsettled(reason)) => {
                    return self.stalled(&reason).await;
                }
            };
            let cand = match self.rebase(&main).await? {
                Rebuilt::Ready(cand) => cand,
                Rebuilt::Stop(end) => return Ok(end),
            };
            match self.cycle(cand).await? {
                Cycle::Merged(landed) => return self.finish(landed).await,
                Cycle::MainMoved => {}
                Cycle::Stop(end) => return Ok(end),
            }
        }
        let reason =
            format!("main moved during each of {MAX_GATES} gates; the batch was not landed");
        self.hold(batch_states::FAILED, reason).await
    }

    async fn settle_open_pr(&self) -> Result<Option<BatchEnd>, PersistError> {
        let Some(url) = self.row.pr_url.clone() else {
            return Ok(None);
        };
        match merged_commit(&self.ctx.runner, &self.ctx.repo.root, &url).await {
            Ok(Some(merge_sha)) => {
                let landed = Landed {
                    merge_sha,
                    note: None,
                };
                Ok(Some(self.finish(landed).await?))
            }
            Ok(None) => {
                let comment = "Wright gates the batch again after a restart.";
                close_pr(&self.ctx.runner, &self.path, &url, comment).await;
                Ok(None)
            }
            Err(e) => {
                tracing::warn!(error = %e, batch = self.row.id, "wright repair: the batch's earlier pull request could not be read");
                Ok(None)
            }
        }
    }

    async fn prepare(&self) -> Result<(), String> {
        worktree::open_batch(
            &self.ctx.runner,
            &self.ctx.repo,
            &self.path,
            self.base_branch(),
            self.current_branch(),
        )
        .await?;
        batch_git::abort_stale(&self.ctx.runner, &self.path).await;
        worktree::restore(&self.ctx.runner, &self.path, "HEAD")
            .await
            .map_err(|e| format!("could not clean the batch worktree: {e}"))
    }

    pub(super) async fn advance(&self, update: BatchUpdate) -> Result<bool, PersistError> {
        self.ctx
            .persist
            .web()
            .advance_wright_fix_batch(&BatchUpdate {
                id: self.row.id,
                ..update
            })
            .await
    }

    pub(super) async fn stalled(&self, reason: &str) -> Result<BatchEnd, PersistError> {
        tracing::warn!(batch = self.row.id, %reason, "wright repair: the batch could not move; it is tried again on the next tick");
        self.advance(BatchUpdate {
            to: "",
            from: &[batch_states::CLOSED, batch_states::GATING],
            reason: Some(format!("will be tried again: {reason}")),
            ..BatchUpdate::default()
        })
        .await?;
        Ok(BatchEnd::Interrupted)
    }

    pub(super) async fn hold(
        &self,
        to: &'static str,
        reason: String,
    ) -> Result<BatchEnd, PersistError> {
        self.advance(BatchUpdate {
            to,
            from: &[batch_states::CLOSED, batch_states::GATING],
            reason: Some(reason.clone()),
            ..BatchUpdate::default()
        })
        .await?;
        tracing::warn!(batch = self.row.id, state = to, %reason, "wright repair: the day's batch was not landed");
        tell_operator(
            &self.ctx.persist,
            &self.row,
            to,
            &reason,
            self.current_branch(),
        )
        .await;
        Ok(BatchEnd::Held)
    }

    pub(super) async fn drop_fixes(&self, lost: &[Lost]) -> Result<(), PersistError> {
        let web = self.ctx.persist.web();
        let mut followups = Vec::new();
        for item in lost {
            if item.fix.is_repair() {
                web.advance_wright_repair(&RepairUpdate {
                    id: item.fix.id,
                    to: states::DROPPED,
                    from: &[states::BATCHED],
                    reason: Some(item.reason.clone()),
                    ..RepairUpdate::default()
                })
                .await?;
            } else {
                followups.push(item.fix.id);
            }
        }
        if !followups.is_empty() {
            web.unbatch_wright_followups(self.row.id, &followups)
                .await?;
        }
        Ok(())
    }

    pub(super) async fn remap(&self, kept: &[(BatchFix, String)]) -> Result<(), PersistError> {
        let pick = |repairs: bool| -> Vec<(i64, String)> {
            kept.iter()
                .filter(|(fix, _)| fix.is_repair() == repairs)
                .map(|(fix, sha)| (fix.id, sha.clone()))
                .collect()
        };
        self.ctx
            .persist
            .web()
            .remap_wright_batch_commits(self.row.id, &pick(true), &pick(false))
            .await
    }

    async fn rebase(&mut self, main: &str) -> Result<Rebuilt, PersistError> {
        let runner = &self.ctx.runner;
        let dropped =
            match batch_git::rebase_dropping(runner, &self.ctx.repo, &self.path, main).await {
                Ok(dropped) => dropped,
                Err(reason) => {
                    return Ok(Rebuilt::Stop(
                        self.hold(batch_states::FAILED, reason).await?,
                    ));
                }
            };
        let commits = match batch_git::commits_between(runner, &self.path, main).await {
            Ok(commits) => commits,
            Err(e) => return Ok(Rebuilt::Stop(self.stalled(&e.to_string()).await?)),
        };
        let fixes = self
            .ctx
            .persist
            .web()
            .wright_fix_batch_fixes(self.row.id)
            .await?;
        let (kept, lost) = split_fixes(fixes, &commits, &dropped, main);
        let on_main = batch_git::fixes_on_main(runner, &self.path, main).await;
        let (merged, lost) = batch_git::already_on_main(lost, &on_main);
        self.drop_fixes(&lost).await?;
        if kept.is_empty() && !merged.is_empty() {
            return Ok(Rebuilt::Stop(self.settle_merged(&merged, main).await?));
        }
        if kept.is_empty() {
            let reason = "every fix of the day was dropped, so nothing is left to land".to_owned();
            return Ok(Rebuilt::Stop(
                self.hold(batch_states::FAILED, reason).await?,
            ));
        }
        self.remap(&[kept, merged].concat()).await?;
        match batch_git::head(runner, &self.path).await {
            Ok(sha) => Ok(Rebuilt::Ready(Candidate {
                branch: self.current_branch().to_owned(),
                base: main.to_owned(),
                sha,
            })),
            Err(e) => Ok(Rebuilt::Stop(self.stalled(&e.to_string()).await?)),
        }
    }

    async fn pushable(&mut self, cand: Candidate) -> Result<Candidate, String> {
        let remote = batch_git::remote_sha(&self.ctx.runner, &self.path, &cand.branch)
            .await
            .map_err(|e| e.to_string())?;
        if remote.is_none_or(|sha| sha == cand.sha) {
            return Ok(cand);
        }
        let name = format!("{}-{}", self.base_branch(), self.branches.len() + 1);
        batch_git::switch_new(&self.ctx.runner, &self.path, &name)
            .await
            .map_err(|e| e.to_string())?;
        self.branches.push(name.clone());
        Ok(Candidate {
            branch: name,
            ..cand
        })
    }

    pub(super) async fn gate(&mut self, cand: Candidate) -> Result<Gated, PersistError> {
        let cand = match self.pushable(cand).await {
            Ok(cand) => cand,
            Err(reason) => return Ok(Gated::Stop(self.stalled(&reason).await?)),
        };
        self.advance(BatchUpdate {
            to: batch_states::GATING,
            from: &[batch_states::CLOSED, batch_states::GATING],
            branch: Some(cand.branch.clone()),
            base_sha: Some(cand.base.clone()),
            ..BatchUpdate::default()
        })
        .await?;
        let result = match gate(&self.ctx.runner, &self.ctx.repo, &self.path, &cand).await {
            Ok(result) => result,
            Err(e) => return Ok(Gated::Stop(self.stalled(&e.to_string()).await?)),
        };
        let exit = result.exit.unwrap_or(-1);
        let (stored, bytes) = kept_output(result.output.clone());
        self.advance(BatchUpdate {
            to: batch_states::GATING,
            from: &[batch_states::GATING],
            gate_exit: Some(exit),
            gate_output: stored,
            gate_output_bytes: Some(bytes),
            ..BatchUpdate::default()
        })
        .await?;
        if result.passed {
            return Ok(Gated::Passed(cand));
        }
        if self.ctx.stopping() {
            return Ok(Gated::Stop(BatchEnd::Interrupted));
        }
        Ok(Gated::Refused(cand, result.output))
    }

    async fn cycle(&mut self, cand: Candidate) -> Result<Cycle, PersistError> {
        let passed = match self.gate(cand).await? {
            Gated::Passed(cand) => cand,
            Gated::Stop(end) => return Ok(Cycle::Stop(end)),
            Gated::Refused(cand, output) => match self.settle_refusal(cand, output).await? {
                Gated::Passed(cand) => cand,
                Gated::Stop(end) => return Ok(Cycle::Stop(end)),
                Gated::Refused(..) => return Ok(Cycle::Stop(BatchEnd::Held)),
            },
        };
        self.publish(passed).await
    }

    async fn publish(&self, cand: Candidate) -> Result<Cycle, PersistError> {
        let fixes = self
            .ctx
            .persist
            .web()
            .wright_fix_batch_fixes(self.row.id)
            .await?;
        let title = format!("Wright fixes for {}", self.row.day);
        let body = pr_body(&self.row, &fixes);
        let runner = &self.ctx.runner;
        let pr = match open_pr(runner, &self.path, &cand.branch, &title, &body).await {
            Ok(pr) => pr,
            Err(LandError::Failed(reason) | LandError::Unsettled(reason)) => {
                let end = self.hold(batch_states::FAILED, reason).await?;
                return Ok(Cycle::Stop(end));
            }
        };
        self.advance(BatchUpdate {
            to: batch_states::GATING,
            from: &[batch_states::GATING],
            pr_url: Some(pr.clone()),
            ..BatchUpdate::default()
        })
        .await?;
        match merge_unless_moved(runner, &self.path, &cand, &pr).await {
            Ok(Merge::Landed(landed)) => Ok(Cycle::Merged(landed)),
            Ok(Merge::MainMoved(now)) => {
                let comment = format!(
                    "Main moved to {now} after this gate; Wright gates the batch again on a new branch."
                );
                close_pr(runner, &self.path, &pr, &comment).await;
                Ok(Cycle::MainMoved)
            }
            Err(LandError::Unsettled(reason)) => Ok(Cycle::Stop(self.stalled(&reason).await?)),
            Err(LandError::Failed(reason)) => {
                Ok(Cycle::Stop(self.hold(batch_states::FAILED, reason).await?))
            }
        }
    }

    pub(super) async fn finish(&self, landed: Landed) -> Result<BatchEnd, PersistError> {
        self.note_landing(&landed).await?;
        let moved = self
            .ctx
            .persist
            .web()
            .land_wright_fix_batch(self.row.id, &landed.merge_sha, None)
            .await?;
        log_landed(self.row.id, &landed.merge_sha, &moved);
        self.remove_worktree().await;
        Ok(BatchEnd::Landed)
    }

    async fn note_landing(&self, landed: &Landed) -> Result<(), PersistError> {
        let Some(note) = &landed.note else {
            return Ok(());
        };
        self.advance(BatchUpdate {
            to: batch_states::GATING,
            from: &[batch_states::CLOSED, batch_states::GATING],
            reason: Some(note.clone()),
            ..BatchUpdate::default()
        })
        .await
        .map(|_| ())
    }

    async fn remove_worktree(&self) {
        let removed =
            worktree::remove_batch(&self.ctx.runner, &self.ctx.repo, &self.path, &self.branches)
                .await;
        if let Err(e) = removed {
            log_remove_failed(self.row.id, &e.to_string());
        }
    }
}
