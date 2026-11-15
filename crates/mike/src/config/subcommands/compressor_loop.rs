use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;

use crate::compress::compressor_loop::{RunOptions, run};
use crate::config::{CliOverrides, load};
use crate::store::persistence::PersistHandle;

const DEFAULT_INSPECT_URL: &str = "http://127.0.0.1:9989";
const STATE_SUBDIR: &str = ".local/state/mike/compressor-loop";

#[derive(Debug, Clone, Args)]
pub struct CompressorLoopArgs {
    #[arg(long, default_value_t = 7)]
    pub days: u32,
    #[arg(long)]
    pub apply: bool,
    #[arg(long)]
    pub evidence: Option<PathBuf>,
}

fn state_dir() -> Result<PathBuf> {
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().join(STATE_SUBDIR))
        .context("could not resolve the home directory for compressor-loop state")
}

fn inspect_url() -> String {
    std::env::var("MIKE_INSPECT_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| DEFAULT_INSPECT_URL.to_string())
}

pub(super) async fn compressor_loop(args: CompressorLoopArgs, cli: CliOverrides) -> Result<()> {
    let config = load::load(cli).context("failed to resolve config for compressor-loop")?;
    let Some(database_url) = config.database_url.as_deref() else {
        anyhow::bail!("compressor-loop requires a database_url");
    };
    let handle = PersistHandle::connect_read_only(database_url)
        .await
        .context("could not connect to the capture database")?;
    let text = run(RunOptions {
        pool: handle.pool().clone(),
        days: i64::from(args.days),
        apply: args.apply,
        evidence: args.evidence,
        inspect_url: inspect_url(),
        state_dir: state_dir()?,
    })
    .await?;
    println!("{text}");
    Ok(())
}

#[cfg(test)]
#[path = "compressor_loop_tests.rs"]
mod compressor_loop_tests;
