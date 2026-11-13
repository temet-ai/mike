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
    let persist = super::eval::connect(cli.clone(), false).await?;
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
    }
}
