use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use tokio_util::task::TaskTracker;

use crate::config::CliOverrides;
use crate::config::persist_bootstrap::PersistLlmUsageSink;
use crate::llm::decisions::record::{DecisionContext, DecisionMode};
use crate::llm::decisions::{DecisionRequest, Question, decision_client};
use crate::llm::{LlmUsageSink, OpenRouterClient};
use crate::settings::SettingsOverlay;
use crate::store::persistence::PersistHandle;

#[derive(Debug, Clone, Subcommand)]
pub enum DecisionsCommand {
    Ask(DecisionsAskArgs),
    Loop(DecisionsLoopArgs),
    Export(DecisionsExportArgs),
    ImportReplay(super::decisions_import::DecisionsImportArgs),
}

#[derive(Debug, Clone, Args)]
pub struct DecisionsLoopArgs {
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Clone, Args)]
pub struct DecisionsExportArgs {
    #[arg(long)]
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    #[arg(long)]
    pub point: Option<String>,
    #[arg(long)]
    pub with_outcomes: bool,
    #[arg(long)]
    pub out: Option<PathBuf>,
}

async fn decisions_loop(args: DecisionsLoopArgs, cli: CliOverrides) -> Result<()> {
    let persist = super::eval::connect(cli, true).await?;
    let now = chrono::Utc::now();
    if !args.force && !crate::steerer::decisions_loop::overdue(&persist, now).await? {
        println!("decisions_loop: a run finished in the last 24 h; nothing to do");
        return Ok(());
    }
    let report = crate::steerer::decisions_loop::run(&persist, now).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

async fn export(args: DecisionsExportArgs, cli: CliOverrides) -> Result<()> {
    use std::io::Write as _;
    let persist = super::eval::connect(cli, false).await?;
    let since = args
        .since
        .unwrap_or(chrono::DateTime::<chrono::Utc>::UNIX_EPOCH);
    let rows = crate::llm::decisions::store(&persist)
        .export_decisions(since, args.point.as_deref(), args.with_outcomes)
        .await?;
    let mut out: Box<dyn std::io::Write> = match &args.out {
        Some(path) => Box::new(std::fs::File::create(path)?),
        None => Box::new(std::io::stdout().lock()),
    };
    for row in &rows {
        writeln!(out, "{row}")?;
    }
    eprintln!("exported {} decision records", rows.len());
    Ok(())
}

#[derive(Debug, Clone, Args)]
pub struct DecisionsAskArgs {
    pub state: PathBuf,
    pub questions: PathBuf,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &PathBuf) -> Result<T> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

pub struct RecordingClient {
    pub client: OpenRouterClient,
    pub overlay: SettingsOverlay,
    tracker: TaskTracker,
}

impl RecordingClient {
    pub async fn connect(cli: CliOverrides, persist: &PersistHandle) -> Result<Self> {
        let config = crate::config::load::load(cli)
            .context("failed to resolve config for mike decisions")?;
        let overlay = crate::settings::apply_db_rows_to_overlay(persist.fetch_settings().await?);
        let key = crate::llm::key::cli_api_key(
            config.steerer.openrouter_api_key.as_ref(),
            &overlay.llm_provider,
            None,
            &overlay.decisions_model,
        )
        .map_err(|e| anyhow::anyhow!("mike decisions: {e}"))?;
        let tracker = TaskTracker::new();
        let sink: Arc<dyn LlmUsageSink> =
            Arc::new(PersistLlmUsageSink::new(persist.clone(), tracker.clone()).await);
        let base = OpenRouterClient::new(
            key,
            overlay.decisions_model.clone(),
            Some(config.steerer.openrouter_base_url.clone()),
            None,
        )?
        .with_usage_sink(sink)
        .with_decision_store(persist.clone());
        Ok(Self {
            client: decision_client(&base, &overlay)?,
            overlay,
            tracker,
        })
    }

    pub async fn flush(self) {
        self.tracker.close();
        self.tracker.wait().await;
    }
}

async fn ask(args: DecisionsAskArgs, cli: CliOverrides) -> Result<()> {
    let state: serde_json::Value = read_json(&args.state)?;
    let questions: BTreeMap<String, Question> = read_json(&args.questions)?;
    let persist = super::eval::connect(cli.clone(), true).await?;
    let recording = RecordingClient::connect(cli, &persist).await?;
    let request = DecisionRequest {
        model: recording.overlay.decisions_model.clone(),
        state,
        questions,
    };
    let context = DecisionContext::new("cli_ask", DecisionMode::Cli, "cli_ask");
    let outcome = recording.client.decide(&request, &context).await;
    recording.flush().await;
    let answer = outcome.result?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "record_id": outcome.record_id,
            "model": answer.model,
            "answers": answer.answers,
            "cost_usd": answer.usage.cost_usd,
            "latency_ms": answer.usage.latency_ms,
        }))?
    );
    Ok(())
}

pub async fn run(command: DecisionsCommand, cli: CliOverrides) -> Result<()> {
    match command {
        DecisionsCommand::Ask(args) => ask(args, cli).await,
        DecisionsCommand::Loop(args) => decisions_loop(args, cli).await,
        DecisionsCommand::Export(args) => export(args, cli).await,
        DecisionsCommand::ImportReplay(args) => super::decisions_import::run(args, cli).await,
    }
}
