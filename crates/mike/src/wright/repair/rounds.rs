use crate::store::persistence::web_repo::WebPersist as _;

use super::already_fixed;
use super::attempt::{AttemptEnd, Fix, Next, abandon, job, release, rewind};
use super::feedback::{self, Refused};
use super::perf::Perf;
use super::port::{RepairEngineer, RepairRun};
use super::runner::CommandRunner;
use super::{RepairContext, states};
use crate::store::persistence::PersistError;
use crate::store::persistence::wright::RepairUpdate;

pub(super) fn halted<R, E, B, T>(ctx: &RepairContext<R, E, B>) -> Option<Next<T>> {
    ctx.stopping().then_some(Next::Stop(AttemptEnd::Ended))
}

async fn round_failed<R, E, B, T>(
    ctx: &RepairContext<R, E, B>,
    fix: &Fix<'_>,
    refused: &Refused,
    round: u32,
    why: &str,
) -> Result<Next<T>, PersistError>
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
) -> Result<Next<Perf>, PersistError>
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
    let job = feedback::feedback_job(
        &job(fix.claim, fix.path, already_fixed::IN_FIRST_ROUND),
        round,
        refused,
    );
    match ctx.engineer.run_in_worktree(&job).await {
        RepairRun {
            not_started: None,
            failure: None,
            perf,
            ..
        } => Ok(Next::Go(perf)),
        RepairRun {
            not_started: Some(why),
            ..
        } => {
            log_round_waits(fix, round, &why);
            rewind(ctx, fix).await;
            Ok(Next::Stop(release(ctx, fix.claim).await?))
        }
        RepairRun {
            failure: Some(why), ..
        } => round_failed(ctx, fix, refused, round, &why).await,
    }
}

fn log_round_waits(fix: &Fix<'_>, round: u32, why: &str) {
    tracing::info!(repair = fix.claim.id, finding = fix.claim.finding_id, round, %why, "wright repair: the round could not start; the finding is repaired again later");
}

fn log_round(fix: &Fix<'_>, round: u32, reason: &str) {
    tracing::info!(repair = fix.claim.id, finding = fix.claim.finding_id, round, %reason, "wright repair: the commit was refused; the engineer edits it again");
}
