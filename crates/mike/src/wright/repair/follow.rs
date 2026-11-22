use crate::store::persistence::web_repo::WebPersist as _;
use std::path::PathBuf;

use super::port::LiveBuild;
use super::runner::{Cmd, CommandRunner, Program, output};
use super::worktree::{self, fetch_main};
use super::{RepairContext, states};
use crate::store::persistence::PersistError;
use crate::store::persistence::wright::{Following, RepairRow, RepairUpdate};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Followed {
    pub(crate) deployed: Vec<i64>,
    pub(crate) healthy: Vec<i64>,
    pub(crate) pending: bool,
}

async fn contains<R: CommandRunner>(
    runner: &R,
    root: &std::path::Path,
    merge: &str,
    live: &str,
) -> bool {
    let cmd = Cmd::new(
        Program::Git,
        root,
        &["merge-base", "--is-ancestor", merge, live],
    );
    output(runner, &cmd).await.is_ok_and(|out| out.ok())
}

async fn deployed<R, E, B>(
    ctx: &RepairContext<R, E, B>,
    landed: Vec<(i64, String)>,
) -> Result<Vec<i64>, PersistError>
where
    R: CommandRunner,
    B: LiveBuild,
{
    let Some(live) = ctx.live.commit() else {
        return Ok(Vec::new());
    };
    if landed.is_empty() {
        return Ok(Vec::new());
    }
    if let Err(e) = fetch_main(&ctx.runner, &ctx.repo.root).await {
        tracing::warn!(error = %e, "wright repair: origin/main could not be fetched");
    }
    let mut carried = Vec::new();
    for (id, merge) in landed {
        if contains(&ctx.runner, &ctx.repo.root, &merge, &live).await {
            carried.push(id);
        }
    }
    if carried.is_empty() {
        return Ok(carried);
    }
    ctx.persist
        .web()
        .mark_wright_repairs_deployed(&carried)
        .await
}

pub(crate) async fn follow<R, E, B>(ctx: &RepairContext<R, E, B>) -> Result<Followed, PersistError>
where
    R: CommandRunner,
    B: LiveBuild,
{
    let Following { landed, deployed } = ctx.persist.web().following_wright_repairs().await?;
    let deployed_before = deployed;
    let landed_count = landed.len();
    let deployed = self::deployed(ctx, landed).await?;
    let healthy = if deployed_before > 0 || !deployed.is_empty() {
        ctx.persist.web().mark_wright_repairs_healthy().await?
    } else {
        Vec::new()
    };
    let pending = landed_count > deployed.len() || deployed_before + deployed.len() > healthy.len();
    Ok(Followed {
        deployed,
        healthy,
        pending,
    })
}

async fn reset_edits<R, E, B>(ctx: &RepairContext<R, E, B>, row: &RepairRow)
where
    R: CommandRunner,
{
    let path = PathBuf::from(&row.worktree);
    let first = row.branches.first().unwrap_or(&row.branch);
    let to = row.base_sha.as_deref().unwrap_or("HEAD");
    let restored = worktree::restore_if_ours(&ctx.runner, &ctx.repo, &path, first, to).await;
    if let Err(e) = restored {
        log_reset_failed(row.id, &e.to_string());
    }
}

fn log_reset_failed(repair: i64, error: &str) {
    tracing::warn!(%error, repair, "wright repair: resetting an interrupted repair's edits failed");
}

async fn settle<R, E, B>(ctx: &RepairContext<R, E, B>, row: &RepairRow) -> Result<(), PersistError>
where
    R: CommandRunner,
{
    let to = if row.state == states::EDITING {
        states::REFUSED
    } else {
        states::GATE_FAILED
    };
    ctx.persist
        .web()
        .advance_wright_repair(&RepairUpdate {
            id: row.id,
            to,
            from: &[states::EDITING, states::PREPARED],
            reason: Some(format!(
                "interrupted: mike stopped while the repair was {}; its edits were taken off the day's worktree and nothing was repeated",
                row.state
            )),
            interrupted: true,
            ..RepairUpdate::default()
        })
        .await?;
    reset_edits(ctx, row).await;
    Ok(())
}

pub(crate) async fn recover<R, E, B>(ctx: &RepairContext<R, E, B>) -> Result<usize, PersistError>
where
    R: CommandRunner,
{
    let rows = ctx
        .persist
        .web()
        .interrupted_wright_repairs(ctx.machine)
        .await?;
    for row in &rows {
        settle(ctx, row).await?;
    }
    Ok(rows.len())
}
