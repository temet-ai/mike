mod checklist;
mod correction;
mod criteria_source;
mod directive;
mod jev;
mod labelled;
mod leaf_judge;
mod phrase;
mod point;
mod run;
mod session;
mod summary;
mod text_origin;
mod turn_judge;
mod turn_review;
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
    #[value(name = "text_origin")]
    TextOrigin,
    #[value(name = "directive")]
    Directive,
    #[value(name = "criteria_source")]
    CriteriaSource,
    #[value(name = "turn_judge_review")]
    TurnJudgeReview,
    #[value(name = "leaf_judge")]
    LeafJudge,
    #[value(name = "diagnostic_phrase")]
    DiagnosticPhrase,
    #[value(name = "steerer_checklist")]
    SteererChecklist,
    #[value(name = "session_judge")]
    SessionJudge,
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
    loaded: impl Future<Output = Result<(Vec<P::Item>, usize)>>,
    jev: &impl Jev,
    out: &Path,
) -> Result<Summary> {
    let (items, skipped) = loaded.await?;
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
            finish::<turn_judge::TurnJudge>(turn_judge::load(persist, since, limit), jev, out).await
        }
        ReplayPointArg::Correction => {
            finish::<correction::Correction>(correction::load(persist, since, limit), jev, out)
                .await
        }
        ReplayPointArg::VerifierPrecheck => {
            finish::<verifier::VerifierPrecheck>(verifier::load(persist, since, limit), jev, out)
                .await
        }
        ReplayPointArg::WakeRecheck => {
            finish::<wake::WakeRecheck>(wake::load(persist, since, limit), jev, out).await
        }
        _ => execute_reviewed(args, persist, jev).await,
    }
}

async fn execute_reviewed(
    args: &DecisionsReplayArgs,
    persist: &PersistHandle,
    jev: &impl Jev,
) -> Result<Summary> {
    let since = args.since.unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
    let limit = args.limit.unwrap_or(i64::MAX);
    let out = args.out.as_path();
    match args.point {
        ReplayPointArg::TextOrigin => {
            finish::<text_origin::TextOrigin>(text_origin::load(persist, since, limit), jev, out)
                .await
        }
        ReplayPointArg::Directive => {
            finish::<directive::Directive>(directive::load(persist, since, limit), jev, out).await
        }
        ReplayPointArg::CriteriaSource => {
            finish::<criteria_source::CriteriaSource>(
                criteria_source::load(persist, since, limit),
                jev,
                out,
            )
            .await
        }
        ReplayPointArg::TurnJudgeReview => {
            finish::<turn_review::TurnReview>(turn_review::load(persist, since, limit), jev, out)
                .await
        }
        ReplayPointArg::LeafJudge => {
            finish::<leaf_judge::LeafJudge>(leaf_judge::load(persist, since, limit), jev, out).await
        }
        ReplayPointArg::DiagnosticPhrase => {
            finish::<phrase::Phrase>(phrase::load(persist, since, limit), jev, out).await
        }
        ReplayPointArg::SteererChecklist => {
            finish::<checklist::Checklist>(checklist::load(persist, since, limit), jev, out).await
        }
        ReplayPointArg::SessionJudge => {
            finish::<session::Session>(session::load(persist, since, limit), jev, out).await
        }
        _ => Err(anyhow::anyhow!("not a hand-labelled replay point")),
    }
}

pub(super) async fn run(args: DecisionsReplayArgs, cli: CliOverrides) -> Result<()> {
    let persist = super::eval::connect(cli.clone(), false).await?;
    let jev = LiveJev::connect(cli, &persist).await?;
    execute(&args, &persist, &jev).await?;
    Ok(())
}
