use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use clap::{Args, Subcommand};

use crate::analysis::embeddings::EmbeddingService;
use crate::config::CliOverrides;
use crate::semantic::SemanticConfig;
use crate::semantic::builder::{BuildReport, Builder};
use crate::semantic::floors_job::{FloorRun, Promote, promote};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::semantic::{EmbedderRow, SemanticWindow};

#[derive(Debug, Clone, Subcommand)]
pub enum SemanticCommand {
    Backfill(BackfillArgs),
    #[command(subcommand)]
    Embedders(EmbeddersCommand),
    #[command(subcommand)]
    Anchors(AnchorsCommand),
    #[command(subcommand)]
    Floors(FloorsCommand),
    Prune(PruneArgs),
    Group(ExecuteArgs),
    #[command(subcommand)]
    Backtest(BacktestCommand),
}

#[derive(Debug, Clone, Args)]
pub struct BackfillArgs {
    #[arg(long)]
    pub from: NaiveDate,
    #[arg(long)]
    pub to: Option<NaiveDate>,
    #[arg(long)]
    pub execute: bool,
}

#[derive(Debug, Clone, Args)]
pub struct ExecuteArgs {
    #[arg(long)]
    pub execute: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum EmbeddersCommand {
    Verify(VerifyArgs),
}

#[derive(Debug, Clone, Args)]
pub struct VerifyArgs {
    #[arg(long)]
    pub since: DateTime<Utc>,
    #[arg(long)]
    pub execute: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum AnchorsCommand {
    Import(AnchorsImportArgs),
}

#[derive(Debug, Clone, Args)]
pub struct AnchorsImportArgs {
    #[arg(long)]
    pub dir: PathBuf,
    #[arg(long)]
    pub execute: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum FloorsCommand {
    Recompute(ExecuteArgs),
    Promote(PromoteArgs),
}

#[derive(Debug, Clone, Args)]
pub struct PromoteArgs {
    #[arg(long)]
    pub pair: String,
    #[arg(long = "from-id")]
    pub from_id: i64,
    #[arg(long)]
    pub rebundle: bool,
    #[arg(long)]
    pub execute: bool,
}

#[derive(Debug, Clone, Args)]
pub struct PruneArgs {
    #[arg(long = "embedder-key")]
    pub embedder_key: String,
    #[arg(long)]
    pub execute: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum BacktestCommand {
    Retrieval(WindowArgs),
    Tagger,
    Drift(WindowArgs),
}

#[derive(Debug, Clone, Args)]
pub struct WindowArgs {
    #[arg(long)]
    pub from: NaiveDate,
    #[arg(long)]
    pub to: NaiveDate,
}

pub(super) struct SemanticCli {
    pub(super) persist: PersistHandle,
    pub(super) embedding: EmbeddingService,
    pub(super) config: SemanticConfig,
}

pub(super) fn day_start(day: NaiveDate) -> DateTime<Utc> {
    day.and_time(NaiveTime::MIN).and_utc()
}

pub(super) fn print<T: serde::Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

impl SemanticCli {
    async fn connect(cli: CliOverrides, write: bool) -> Result<Self> {
        let config = crate::config::load::load(cli.clone())
            .context("failed to resolve config for mike semantic")?;
        let persist = super::eval::connect(cli, write).await?;
        let rows = persist.fetch_settings().await?;
        let embedding = EmbeddingService::new(&config.embedding);
        for (key, value) in rows.iter().filter(|(k, _)| k.starts_with("embedding.")) {
            embedding.apply_setting(key, value);
        }
        Ok(Self {
            persist,
            embedding,
            config: SemanticConfig::from_rows(&rows),
        })
    }

    pub(super) async fn current(&self) -> Result<EmbedderRow> {
        self.persist
            .current_semantic_embedder()
            .await?
            .context("no embedder is registered yet: run the worker, or mike semantic backfill --execute, first")
    }

    async fn embedder_for_write(&self) -> Result<EmbedderRow> {
        Ok(crate::semantic::check::ensure_embedder(
            &self.persist,
            &self.embedding,
            self.config.allow_remote_embedding,
        )
        .await?)
    }
}

async fn backfill(c: &SemanticCli, args: &BackfillArgs) -> Result<()> {
    let window = |after_turn| SemanticWindow {
        from: Some(day_start(args.from)),
        until: args.to.map(day_start),
        after_turn,
        limit: c.config.batch_turns,
    };
    let embedder = if args.execute {
        c.embedder_for_write().await?
    } else {
        c.current().await?
    };
    let builder = Builder {
        persist: &c.persist,
        embedding: &c.embedding,
        config: &c.config,
        embedder: &embedder,
    };
    let mut total = BuildReport::default();
    let mut after = 0_i64;
    loop {
        let turns = c
            .persist
            .turns_needing_semantic_items(
                &embedder.embedder_key,
                crate::semantic::BUILDER_VERSION,
                &window(after),
            )
            .await?;
        let Some(last) = turns.last().map(|t| t.turn_id) else {
            break;
        };
        after = last;
        total.turns += turns.len();
        if args.execute {
            let r = builder.build_turns(&turns).await?;
            total.copied += r.copied;
            total.embedded += r.embedded;
            total.egress_blocked |= r.egress_blocked;
        }
    }
    if args.execute {
        total.directives = builder.build_directives(10_000).await?;
    }
    print(
        &serde_json::json!({ "embedder_key": embedder.embedder_key, "dry_run": !args.execute, "report": total }),
    )
}

async fn floors(c: &SemanticCli, cmd: &FloorsCommand) -> Result<()> {
    let embedder = c.current().await?;
    match cmd {
        FloorsCommand::Recompute(a) => {
            let run = FloorRun {
                persist: &c.persist,
                embedder: &embedder,
                config: &c.config,
                seed: "floors-recompute",
                execute: a.execute,
            };
            print(&run.compute_all().await?)
        }
        FloorsCommand::Promote(p) => {
            let llm_facing: Vec<&str> =
                if c.config.relapse_evidence == crate::semantic::config::RelapseEvidence::Input {
                    crate::semantic::floors::SIMILAR_CORRECTIONS.to_vec()
                } else {
                    Vec::new()
                };
            let result = promote(
                &c.persist,
                &Promote {
                    pair: &p.pair,
                    from_id: p.from_id,
                    rebundle: p.rebundle,
                    execute: p.execute,
                    llm_facing_pairs: &llm_facing,
                },
            )
            .await?;
            print(&result)
        }
    }
}

async fn prune(c: &SemanticCli, a: &PruneArgs) -> Result<()> {
    let current = c.persist.current_semantic_embedder().await?;
    if current
        .as_ref()
        .is_some_and(|e| e.embedder_key == a.embedder_key)
    {
        bail!(
            "refusing to prune the current embedder key {}",
            a.embedder_key
        );
    }
    if !a.execute {
        println!(
            "Dry run: would delete the candidates, assignments, items, drift rows and floors of {} and keep its retired embedder row.",
            a.embedder_key
        );
        return Ok(());
    }
    match c.persist.prune_semantic_embedder(&a.embedder_key).await? {
        Some(report) => print(&report),
        None => bail!("no retired embedder with key {}", a.embedder_key),
    }
}

pub(super) async fn run(cmd: SemanticCommand, cli: CliOverrides) -> Result<()> {
    let write = matches!(
        &cmd,
        SemanticCommand::Backfill(BackfillArgs { execute: true, .. })
            | SemanticCommand::Embedders(EmbeddersCommand::Verify(VerifyArgs {
                execute: true,
                ..
            }))
            | SemanticCommand::Anchors(AnchorsCommand::Import(AnchorsImportArgs {
                execute: true,
                ..
            }))
            | SemanticCommand::Floors(_)
            | SemanticCommand::Prune(PruneArgs { execute: true, .. })
            | SemanticCommand::Group(ExecuteArgs { execute: true })
    );
    let c = SemanticCli::connect(cli, write).await?;
    match &cmd {
        SemanticCommand::Backfill(a) => backfill(&c, a).await,
        SemanticCommand::Embedders(EmbeddersCommand::Verify(a)) => {
            let report = crate::semantic::check::verify_copy_window(
                &c.persist,
                &c.embedding,
                a.since,
                a.execute,
                c.config.allow_remote_embedding,
            )
            .await?;
            print(&report)
        }
        SemanticCommand::Floors(f) => floors(&c, f).await,
        SemanticCommand::Prune(a) => prune(&c, a).await,
        other => super::semantic_backtest::run(&c, other).await,
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::SemanticCommand;

    #[derive(Parser)]
    struct Cli {
        #[command(subcommand)]
        command: SemanticCommand,
    }

    #[test]
    fn backfill_and_prune_default_to_dry_run() -> anyhow::Result<()> {
        let parsed = Cli::try_parse_from(["x", "backfill", "--from", "2026-07-27"])?;
        assert!(matches!(parsed.command, SemanticCommand::Backfill(ref a) if !a.execute));
        let parsed = Cli::try_parse_from(["x", "prune", "--embedder-key", "k"])?;
        assert!(matches!(parsed.command, SemanticCommand::Prune(ref a) if !a.execute));
        let parsed = Cli::try_parse_from([
            "x",
            "floors",
            "promote",
            "--pair",
            "prompt:corr",
            "--from-id",
            "3",
        ])?;
        assert!(
            matches!(parsed.command, SemanticCommand::Floors(super::FloorsCommand::Promote(ref a)) if !a.execute && !a.rebundle)
        );
        Ok(())
    }
}
