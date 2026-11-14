use crate::store::persistence::web_repo::WebPersist as _;
use std::path::PathBuf;

use super::land::merged_commit;
use super::port::LiveBuild;
use super::runner::{Cmd, CommandRunner, Program, output};
use super::worktree::{self, fetch_main};
use super::{RepairContext, states};
use crate::store::persistence::PersistError;
use crate::store::persistence::wright::{RepairRow, RepairUpdate};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Followed {
    pub(crate) deployed: Vec<i64>,
    pub(crate) healthy: Vec<i64>,
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

async fn deployed<R, E, B>(ctx: &RepairContext<R, E, B>) -> Result<Vec<i64>, PersistError>
where
    R: CommandRunner,
    B: LiveBuild,
{
    let landed = ctx.persist.web().landed_wright_repairs().await?;
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
    let deployed = deployed(ctx).await?;
    let healthy = ctx.persist.web().mark_wright_repairs_healthy().await?;
    Ok(Followed { deployed, healthy })
}

async fn settle<R, E, B>(ctx: &RepairContext<R, E, B>, row: &RepairRow) -> Result<(), PersistError>
where
    R: CommandRunner,
{
    let merged = match (row.state.as_str(), row.pr_url.as_deref()) {
        (states::GATING, Some(url)) => merged_commit(&ctx.runner, &ctx.repo.root, url)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, repair = row.id, "wright repair: the pull request could not be read");
                None
            }),
        _ => None,
    };
    let update = match merged {
        Some(merge_sha) => RepairUpdate {
            to: states::LANDED,
            merge_sha: Some(merge_sha),
            ..RepairUpdate::default()
        },
        None => RepairUpdate {
            to: if row.state == states::EDITING {
                states::REFUSED
            } else {
                states::GATE_FAILED
            },
            reason: Some(format!(
                "interrupted: mike stopped while the repair was {}; nothing was repeated",
                row.state
            )),
            ..RepairUpdate::default()
        },
    };
    ctx.persist
        .web()
        .advance_wright_repair(&RepairUpdate {
            id: row.id,
            from: &[states::EDITING, states::PREPARED, states::GATING],
            ..update
        })
        .await?;
    let path = PathBuf::from(&row.worktree);
    let first = row.branches.first().unwrap_or(&row.branch);
    if let Err(e) = worktree::close(&ctx.runner, &ctx.repo, &path, first).await {
        tracing::warn!(error = %e, repair = row.id, "wright repair: removing an interrupted worktree failed");
    }
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
