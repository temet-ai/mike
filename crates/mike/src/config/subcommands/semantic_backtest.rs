use anyhow::Result;

use super::semantic::{
    AnchorsCommand, BacktestCommand, SemanticCli, SemanticCommand, day_start, print,
};
use crate::semantic::anchors::{AnchorImport, load_plan};
use crate::semantic::backtest;
use crate::semantic::tag_job::group_weekly;

pub(super) async fn run(c: &SemanticCli, cmd: &SemanticCommand) -> Result<()> {
    match cmd {
        SemanticCommand::Anchors(AnchorsCommand::Import(a)) => {
            let plan = load_plan(&a.dir)?;
            let embedder = if a.execute {
                crate::semantic::check::ensure_embedder(
                    &c.persist,
                    &c.embedding,
                    c.config.allow_remote_embedding,
                )
                .await?
            } else {
                c.current().await?
            };
            let import = AnchorImport {
                persist: &c.persist,
                embedding: &c.embedding,
                embedder: &embedder,
                allow_remote: c.config.allow_remote_embedding,
            };
            print(&import.run(&plan, a.execute).await?)
        }
        SemanticCommand::Group(a) => {
            let embedder = c.current().await?;
            let candidates = group_weekly(&c.persist, &c.config, &embedder, a.execute).await?;
            print(&serde_json::json!({
                "dry_run": !a.execute,
                "candidates": candidates.iter().map(|g| serde_json::json!({
                    "members": g.member_item_ids,
                    "sessions": g.session_count,
                    "medoid_item_id": g.medoid_item_id,
                })).collect::<Vec<_>>(),
            }))
        }
        SemanticCommand::Backtest(b) => backtest_cmd(c, b).await,
        _ => Ok(()),
    }
}

async fn backtest_cmd(c: &SemanticCli, b: &BacktestCommand) -> Result<()> {
    let embedder = c.current().await?;
    match b {
        BacktestCommand::Retrieval(w) => print(
            &backtest::retrieval(
                &c.persist,
                &c.config,
                &embedder,
                (day_start(w.from), day_start(w.to)),
            )
            .await?,
        ),
        BacktestCommand::Tagger => {
            print(&backtest::tagger(&c.persist, &c.config, &embedder).await?)
        }
        BacktestCommand::Drift(w) => {
            print(&backtest::drift(&c.persist, &c.config, &embedder, (w.from, w.to)).await?)
        }
    }
}
