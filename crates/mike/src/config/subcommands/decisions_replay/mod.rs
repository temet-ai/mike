mod correction;
mod jev;
mod point;
mod run;
mod summary;
mod turn_judge;
mod verifier;
mod wake;

#[cfg(test)]
mod run_tests;
#[cfg(test)]
mod stub_tests;
#[cfg(test)]
mod summary_tests;

use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Args, ValueEnum};

use self::jev::{Jev, LiveJev};
use self::point::ReplayPoint;
use self::run::Replayed;
use self::summary::Summary;
use crate::config::CliOverrides;
use crate::store::persistence::PersistHandle;

pub(super) use self::point::known_point;
pub(super) use self::run::SCHEMA;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ReplayPointArg {
    #[value(name = "turn_judge")]
    TurnJudge,
    #[value(name = "correction")]
    Correction,
    #[value(name = "verifier_precheck")]
    VerifierPrecheck,
    #[value(name = "wake_recheck")]
    WakeRecheck,
}

#[derive(Debug, Clone, Args)]
pub struct DecisionsReplayArgs {
    #[arg(long, value_enum)]
    pub point: ReplayPointArg,
    #[arg(long)]
    pub limit: Option<i64>,
    #[arg(long)]
    pub since: Option<DateTime<Utc>>,
    #[arg(long)]
    pub out: PathBuf,
}

fn write_lines(path: &Path, replayed: &Replayed) -> Result<()> {
    let mut file =
        std::fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
    for outcome in &replayed.outcomes {
        writeln!(file, "{}", outcome.line)?;
    }
    Ok(())
}

async fn finish<P: ReplayPoint>(
    loaded: (Vec<P::Item>, usize),
    jev: &impl Jev,
    out: &Path,
) -> Result<Summary> {
    let (items, skipped) = loaded;
    if skipped > 0 {
        eprintln!("{skipped} items had no usable context and were skipped");
    }
    let replayed = run::replay::<P>(&items, jev).await;
    write_lines(out, &replayed)?;
    let summary = summary::summarize(P::POINT, &replayed.outcomes, replayed.stopped);
    summary::print(&summary);
    Ok(summary)
}

async fn execute(
    args: &DecisionsReplayArgs,
    persist: &PersistHandle,
    jev: &impl Jev,
) -> Result<Summary> {
    let since = args.since.unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
    let limit = args.limit.unwrap_or(i64::MAX);
    let out = args.out.as_path();
    match args.point {
        ReplayPointArg::TurnJudge => {
            let loaded = turn_judge::load(persist, since, limit).await?;
            finish::<turn_judge::TurnJudge>(loaded, jev, out).await
        }
        ReplayPointArg::Correction => {
            let loaded = correction::load(persist, since, limit).await?;
            finish::<correction::Correction>(loaded, jev, out).await
        }
        ReplayPointArg::VerifierPrecheck => {
            let loaded = verifier::load(persist, since, limit).await?;
            finish::<verifier::VerifierPrecheck>(loaded, jev, out).await
        }
        ReplayPointArg::WakeRecheck => {
            let loaded = wake::load(persist, since, limit).await?;
            finish::<wake::WakeRecheck>(loaded, jev, out).await
        }
    }
}

pub(super) async fn run(args: DecisionsReplayArgs, cli: CliOverrides) -> Result<()> {
    let persist = super::eval::connect(cli.clone(), false).await?;
    let jev = LiveJev::connect(cli, &persist).await?;
    execute(&args, &persist, &jev).await?;
    Ok(())
}
