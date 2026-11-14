use crate::store::persistence::steerer_repo::SteererPersist as _;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Duration, Utc};
use clap::{Args, Subcommand};

use crate::api::steering_guard::{MetricsQuery, guard_metrics_view};
use crate::config::CliOverrides;
use crate::llm::{LlmRole, OpenRouterClient};
use crate::steerer::guard::Question;
use crate::steerer::guard::policy::{GuardConfig, writable_mode};
use crate::steerer::guard::replay::{
    REPLAY_BATCH, ReplayRequest, TruthLabel, replay_full, replay_gates,
};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::steering::guard::ReplayWindow;

#[derive(Debug, Clone, Subcommand)]
pub enum GuardCommand {
    Calibration(GuardCalibrationArgs),
    Report(GuardReportArgs),
    ReplayGates(GuardReplayGatesArgs),
    Replay(GuardReplayArgs),
    PrecheckReplay(super::guard_precheck_replay::PrecheckReplayArgs),
    Mode(GuardModeArgs),
}

#[derive(Debug, Clone, Args)]
pub struct GuardReportArgs {
    #[arg(long)]
    pub since: Option<DateTime<Utc>>,
    #[arg(long)]
    pub until: Option<DateTime<Utc>>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct GuardReplayGatesArgs {
    #[arg(long)]
    pub since: DateTime<Utc>,
    #[arg(long)]
    pub until: Option<DateTime<Utc>>,
    #[arg(long)]
    pub turns: Option<PathBuf>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct GuardReplayArgs {
    #[arg(long)]
    pub since: DateTime<Utc>,
    #[arg(long)]
    pub until: Option<DateTime<Utc>>,
    #[arg(long)]
    pub turns: Option<PathBuf>,
    #[arg(long)]
    pub labels: Option<PathBuf>,
    #[arg(long)]
    pub calibration: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct GuardCalibrationArgs {
    #[arg(long = "as-of")]
    pub as_of: Option<DateTime<Utc>>,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Args)]
pub struct GuardModeArgs {
    pub question: String,
    pub mode: String,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &PathBuf) -> Result<T> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

async fn guard_steerer(persist: &PersistHandle) -> Result<(i64, serde_json::Value)> {
    let row = persist
        .steerer()
        .guard_steerer()
        .await?
        .context("the turn-guard steerer row is not seeded; run the migrations first")?;
    Ok((row.id, row.config))
}

async fn replay_request<'a>(
    persist: &'a PersistHandle,
    since: DateTime<Utc>,
    until: Option<DateTime<Utc>>,
    turns: Option<&PathBuf>,
) -> Result<(ReplayRequest<'a>, String)> {
    let (_, config) = guard_steerer(persist).await?;
    let turn_ids: Vec<i64> = turns.map(read_json).transpose()?.unwrap_or_default();
    let overlay = crate::settings::apply_db_rows_to_overlay(persist.fetch_settings().await?);
    let pattern = persist
        .steerer()
        .verifier_pattern()
        .await?
        .unwrap_or_default();
    let window = ReplayWindow {
        since,
        until: until.unwrap_or_else(Utc::now),
        after_turn: 0,
        limit: if turn_ids.is_empty() {
            REPLAY_BATCH
        } else {
            i64::try_from(turn_ids.len()).unwrap_or(REPLAY_BATCH)
        },
        turn_ids,
    };
    Ok((
        ReplayRequest {
            persist,
            window,
            verifier_pattern: "",
            turn_judge_live: overlay.turn_judge_live_bundle_id > 0,
            config: GuardConfig::from_json(&config),
            calibration: None,
        },
        pattern,
    ))
}

fn print<T: serde::Serialize + std::fmt::Debug>(value: &T, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        println!("{value:#?}");
    }
    Ok(())
}

async fn report(args: GuardReportArgs, cli: CliOverrides) -> Result<()> {
    let persist = super::eval::connect(cli, false).await?;
    let (steerer_id, _) = guard_steerer(&persist).await?;
    let view = guard_metrics_view(
        &persist,
        steerer_id,
        &MetricsQuery {
            since: args.since.or_else(|| Some(Utc::now() - Duration::days(14))),
            until: args.until,
            question: None,
        },
    )
    .await?;
    print(&view, args.json)?;
    if view.audit.audit_set_fires > 0 {
        eprintln!(
            "audit: {} would-fire(s) on seat or template sessions",
            view.audit.audit_set_fires
        );
        std::process::exit(1);
    }
    Ok(())
}

async fn replay_gates_cmd(args: GuardReplayGatesArgs, cli: CliOverrides) -> Result<()> {
    let persist = super::eval::connect(cli, false).await?;
    let (mut req, pattern) =
        replay_request(&persist, args.since, args.until, args.turns.as_ref()).await?;
    req.verifier_pattern = &pattern;
    let report = replay_gates(&req).await?;
    print(&report, args.json)
}

fn eval_client(
    config: &crate::config::ProxyConfig,
    overlay: &crate::settings::SettingsOverlay,
) -> Result<OpenRouterClient> {
    let model = config.steerer.model.clone();
    let key = crate::llm::key::cli_api_key(
        config.steerer.openrouter_api_key.as_ref(),
        &overlay.llm_provider,
        None,
        &model,
    )
    .map_err(|e| anyhow::anyhow!("mike guard replay: {e}"))?;
    Ok(OpenRouterClient::new(key, model, None, None)?.with_attribution(LlmRole::Eval, None))
}

async fn replay_cmd(args: GuardReplayArgs, cli: CliOverrides) -> Result<()> {
    let config = crate::config::load::load(cli.clone())
        .context("failed to resolve config for mike guard replay")?;
    let persist = super::eval::connect(cli, false).await?;
    let labels: Vec<TruthLabel> = args
        .labels
        .as_ref()
        .map(read_json)
        .transpose()?
        .unwrap_or_default();
    let (mut req, pattern) =
        replay_request(&persist, args.since, args.until, args.turns.as_ref()).await?;
    req.verifier_pattern = &pattern;
    if args.calibration {
        req.calibration =
            Some(crate::cross_session::CrossSessionSnapshot::load_now(&persist).await?);
    }
    let overlay = crate::settings::apply_db_rows_to_overlay(persist.fetch_settings().await?);
    let report = replay_full(&eval_client(&config, &overlay)?, &req, &labels).await?;
    print(&report, args.json)?;
    if report.audit_set_fires > 0 {
        std::process::exit(1);
    }
    Ok(())
}

async fn mode(args: GuardModeArgs, cli: CliOverrides) -> Result<()> {
    let Some(question) = Question::parse(&args.question) else {
        bail!(
            "question {:?} is not one of relapse|unbacked_wait|authorised_handback|unobserved_done",
            args.question
        );
    };
    let mode = writable_mode(&args.mode).map_err(anyhow::Error::msg)?;
    let persist = super::eval::connect(cli, true).await?;
    let (steerer_id, _) = guard_steerer(&persist).await?;
    if let Some(current) = persist
        .steerer()
        .policy_row(steerer_id, question.code())
        .await?
        && matches!(current.mode.as_str(), "append" | "wake")
    {
        bail!(
            "{} is {}; demote it with mike steer-questions demote",
            question.code(),
            current.mode
        );
    }
    let row = persist
        .steerer()
        .set_question_mode(steerer_id, question.code(), mode.code(), "cli")
        .await?;
    println!("{} -> {} ({})", row.question, row.mode, row.updated_at);
    Ok(())
}

async fn calibration(args: GuardCalibrationArgs, cli: CliOverrides) -> Result<()> {
    let persist = super::eval::connect(cli, false).await?;
    let snapshot = match args.as_of {
        Some(at) => crate::cross_session::CrossSessionSnapshot::load_as_of(&persist, at).await?,
        None => crate::cross_session::CrossSessionSnapshot::load_now(&persist).await?,
    };
    print(&snapshot, args.json)
}

pub async fn run(command: GuardCommand, cli: CliOverrides) -> Result<()> {
    match command {
        GuardCommand::Calibration(args) => calibration(args, cli).await,
        GuardCommand::Report(args) => report(args, cli).await,
        GuardCommand::ReplayGates(args) => replay_gates_cmd(args, cli).await,
        GuardCommand::Replay(args) => replay_cmd(args, cli).await,
        GuardCommand::PrecheckReplay(args) => super::guard_precheck_replay::run(args, cli).await,
        GuardCommand::Mode(args) => mode(args, cli).await,
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::GuardCommand;

    #[derive(Parser)]
    struct Cli {
        #[command(subcommand)]
        command: GuardCommand,
    }

    #[test]
    fn guard_cli_parses_every_subcommand() -> anyhow::Result<()> {
        assert!(matches!(
            Cli::try_parse_from(["x", "report"])?.command,
            GuardCommand::Report(_)
        ));
        assert!(matches!(
            Cli::try_parse_from(["x", "replay-gates", "--since", "2026-07-27T00:00:00Z"])?.command,
            GuardCommand::ReplayGates(_)
        ));
        assert!(matches!(
            Cli::try_parse_from(["x", "mode", "relapse", "off"])?.command,
            GuardCommand::Mode(_)
        ));
        assert!(matches!(
            Cli::try_parse_from(["x", "precheck-replay", "--dump", "/tmp/x.jsonl"])?.command,
            GuardCommand::PrecheckReplay(_)
        ));
        Ok(())
    }
}
