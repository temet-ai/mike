use crate::store::persistence::web_repo::WebPersist as _;
use std::path::Path;

use chrono::{DateTime, NaiveDate, Utc};

use super::batch_land::{BatchEnd, land};
use super::port::RepairEngineer;
use super::runner::CommandRunner;
use super::worktree::{self, batch_branch};
use super::{RepairContext, batch_states};
use crate::store::persistence::PersistError;
use crate::store::persistence::wright::FixBatchRow;

pub(super) async fn close_due<R, E, B>(
    ctx: &RepairContext<R, E, B>,
    today: NaiveDate,
) -> Result<usize, PersistError>
where
    R: CommandRunner,
{
    let closed = ctx
        .persist
        .web()
        .close_due_wright_fix_batches(today)
        .await?;
    for batch in closed.iter().filter(|batch| !batch.kept) {
        let branches = [batch.branch.clone()];
        let path = Path::new(&batch.worktree);
        if let Err(e) = worktree::remove_batch(&ctx.runner, &ctx.repo, path, &branches).await {
            tracing::warn!(error = %e, batch = batch.id, "wright repair: removing an empty batch's worktree failed");
        }
    }
    Ok(closed.len())
}

pub(super) async fn limited<R, E, B>(
    ctx: &RepairContext<R, E, B>,
) -> Result<Option<DateTime<Utc>>, PersistError> {
    let until = ctx.persist.web().wright_engineer_blocked_until().await?;
    Ok(until.filter(|until| *until > Utc::now()))
}

fn needs_engineer(row: &FixBatchRow) -> bool {
    row.state == batch_states::GATING && row.gate_exit.is_some_and(|exit| exit != 0)
}

pub(super) async fn land_due<R, E, B>(
    ctx: &RepairContext<R, E, B>,
) -> Result<Vec<(i64, BatchEnd)>, PersistError>
where
    R: CommandRunner,
    E: RepairEngineer,
{
    let due = ctx
        .persist
        .web()
        .wright_fix_batches_to_land(ctx.machine)
        .await?;
    let waiting = due.iter().any(needs_engineer) && limited(ctx).await?.is_some();
    let mut ended = Vec::new();
    for row in due {
        if waiting && needs_engineer(&row) {
            continue;
        }
        if ctx.stopping() && !ended.is_empty() {
            break;
        }
        let id = row.id;
        ended.push((id, land(ctx, row).await?));
    }
    Ok(ended)
}

pub(super) async fn open_today<R, E, B>(
    ctx: &RepairContext<R, E, B>,
    today: NaiveDate,
) -> Result<Option<FixBatchRow>, PersistError>
where
    R: CommandRunner,
{
    let web = ctx.persist.web();
    let older = web.open_wright_fix_batch(None).await?;
    if older.is_some_and(|batch| batch.day < today) {
        return Ok(None);
    }
    let branch = batch_branch(today);
    let path = ctx.repo.batch_dir(today);
    let row = web
        .ensure_wright_fix_batch(today, &branch, &path.display().to_string(), ctx.machine)
        .await?;
    if row.state != batch_states::OPEN {
        return Ok(None);
    }
    match worktree::open_batch(&ctx.runner, &ctx.repo, &path, &branch, &row.branch).await {
        Ok(_) => Ok(Some(row)),
        Err(reason) => {
            tracing::warn!(batch = row.id, %reason, "wright repair: today's batch worktree could not be opened");
            Ok(None)
        }
    }
}
