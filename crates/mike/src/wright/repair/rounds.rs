use crate::store::persistence::web_repo::WebPersist as _;
use std::path::Path;

use super::attempt::{AttemptEnd, Next, end, job};
use super::feedback::{self, Refused};
use super::port::{RepairEngineer, RepairRun};
use super::runner::CommandRunner;
use super::{RepairContext, states};
use crate::store::persistence::PersistError;
use crate::store::persistence::wright::{ClaimedRepair, RepairUpdate};

pub(super) fn halted<R, E, B, T>(ctx: &RepairContext<R, E, B>) -> Option<Next<T>> {
    ctx.stopping().then_some(Next::Stop(AttemptEnd::Ended))
}

async fn reopen_failed<R, E, B>(
    ctx: &RepairContext<R, E, B>,
    claim: &ClaimedRepair,
    refused: &Refused,
    round: u32,
    error: impl std::fmt::Display,
) -> Result<Next<String>, PersistError>
where
    R: CommandRunner,
{
    if let Some(stop) = halted(ctx) {
        return Ok(stop);
    }
    let reason = format!(
        "{}; the change could not be reopened for round {round}: {error}",
        refused.reason
    );
    Ok(Next::Stop(
        end(ctx, claim, states::GATE_FAILED, reason, true).await?,
    ))
}

async fn round_failed<R, E, B>(
    ctx: &RepairContext<R, E, B>,
    claim: &ClaimedRepair,
    refused: &Refused,
    round: u32,
    why: &str,
) -> Result<Next<String>, PersistError>
where
    R: CommandRunner,
{
    if let Some(stop) = halted(ctx) {
        return Ok(stop);
    }
    let reason = format!(
        "{}; round {round}'s engineer run did not finish: {why}",
        refused.reason
    );
    Ok(Next::Stop(
        end(ctx, claim, states::GATE_FAILED, reason, true).await?,
    ))
}

pub(super) async fn again<R, E, B>(
    ctx: &RepairContext<R, E, B>,
    claim: &ClaimedRepair,
    path: &Path,
    base: String,
    round: u32,
    refused: &Refused,
) -> Result<Next<String>, PersistError>
where
    R: CommandRunner,
    E: RepairEngineer,
{
    if let Some(stop) = halted(ctx) {
        return Ok(stop);
    }
    let base = match feedback::reopen(&ctx.runner, path, refused).await {
        Ok(reopened) => reopened.unwrap_or(base),
        Err(e) => return reopen_failed(ctx, claim, refused, round, e).await,
    };
    ctx.persist
        .web()
        .advance_wright_repair(&RepairUpdate {
            id: claim.id,
            to: states::EDITING,
            from: &[states::EDITING, states::PREPARED, states::GATING],
            base_sha: Some(base.clone()),
            reason: Some(format!("round {round}: {}", refused.reason)),
            ..RepairUpdate::default()
        })
        .await?;
    log_round(claim, round, &refused.reason);
    let job = feedback::feedback_job(&job(claim, path), round, refused);
    match ctx.engineer.run_in_worktree(&job).await {
        RepairRun {
            not_started: None,
            failure: None,
        } => Ok(Next::Go(base)),
        RepairRun {
            failure: Some(why), ..
        }
        | RepairRun {
            not_started: Some(why),
            ..
        } => round_failed(ctx, claim, refused, round, &why).await,
    }
}

fn log_round(claim: &ClaimedRepair, round: u32, reason: &str) {
    tracing::info!(repair = claim.id, finding = claim.finding_id, round, %reason, "wright repair: the change was refused; the engineer edits it again");
}
