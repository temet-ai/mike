use crate::store::persistence::web_repo::WebPersist as _;

use super::attempt::{AttemptEnd, Fix, Next, abandon, job};
use super::feedback::{self, Refused};
use super::port::{RepairEngineer, RepairRun};
use super::runner::CommandRunner;
use super::{RepairContext, states};
use crate::store::persistence::PersistError;
use crate::store::persistence::wright::RepairUpdate;

pub(super) fn halted<R, E, B, T>(ctx: &RepairContext<R, E, B>) -> Option<Next<T>> {
    ctx.stopping().then_some(Next::Stop(AttemptEnd::Ended))
}

async fn round_failed<R, E, B>(
    ctx: &RepairContext<R, E, B>,
    fix: &Fix<'_>,
    refused: &Refused,
    round: u32,
    why: &str,
) -> Result<Next<()>, PersistError>
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
        abandon(ctx, fix, states::GATE_FAILED, reason).await?,
    ))
}

pub(super) async fn again<R, E, B>(
    ctx: &RepairContext<R, E, B>,
    fix: &Fix<'_>,
    round: u32,
    refused: &Refused,
) -> Result<Next<()>, PersistError>
where
    R: CommandRunner,
    E: RepairEngineer,
{
    if let Some(stop) = halted(ctx) {
        return Ok(stop);
    }
    ctx.persist
        .web()
        .advance_wright_repair(&RepairUpdate {
            id: fix.claim.id,
            to: states::EDITING,
            from: &[states::EDITING, states::PREPARED],
            base_sha: Some(fix.base.to_owned()),
            reason: Some(format!("round {round}: {}", refused.reason)),
            ..RepairUpdate::default()
        })
        .await?;
    log_round(fix, round, &refused.reason);
    let job = feedback::feedback_job(&job(fix.claim, fix.path), round, refused);
    match ctx.engineer.run_in_worktree(&job).await {
        RepairRun {
            not_started: None,
            failure: None,
        } => Ok(Next::Go(())),
        RepairRun {
            failure: Some(why), ..
        }
        | RepairRun {
            not_started: Some(why),
            ..
        } => round_failed(ctx, fix, refused, round, &why).await,
    }
}

fn log_round(fix: &Fix<'_>, round: u32, reason: &str) {
    tracing::info!(repair = fix.claim.id, finding = fix.claim.finding_id, round, %reason, "wright repair: the commit was refused; the engineer edits it again");
}
