use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use serde_json::Value;

use crate::config::steerer::SteererConfig;
use crate::lesson_proposer::{self, ProposerDeps, ProposerSettings};
use crate::llm::key::OpenRouterKeySlot;
use crate::llm::{LlmRole, LlmUsageSink, OpenRouterClient};
use crate::memory_workers::{self, LESSON_PROPOSER, Skip};
use crate::semantic::SemanticConfig;
use crate::store::persistence::PersistHandle;

use super::sweep::SweepContext;

pub const PROPOSER_EVERY_TICKS: u64 = 5;

pub fn is_proposer_tick(tick: u64) -> bool {
    tick > 0 && tick.is_multiple_of(PROPOSER_EVERY_TICKS)
}

fn proposer_client(
    ctx: &SweepContext<'_>,
    settings: &ProposerSettings,
) -> Option<OpenRouterClient> {
    lesson_client(
        ctx.config,
        &settings.effective_model,
        ctx.usage_sink,
        ctx.key_slot,
    )
}

pub fn lesson_client(
    config: &SteererConfig,
    model: &str,
    usage_sink: Option<&Arc<dyn LlmUsageSink>>,
    key_slot: &OpenRouterKeySlot,
) -> Option<OpenRouterClient> {
    let base = OpenRouterClient::new(
        String::new(),
        model.to_owned(),
        Some(config.openrouter_base_url.clone()),
        Some(Duration::from_secs(config.request_timeout_secs)),
    )
    .ok()?
    .with_attribution(LlmRole::LessonProposer, usage_sink);
    crate::llm::key::effective_client(&base, key_slot)
}

pub async fn semantic_config(persist: &PersistHandle) -> SemanticConfig {
    persist
        .fetch_settings()
        .await
        .map(|rows| SemanticConfig::from_rows(&rows))
        .unwrap_or_default()
}

pub async fn run_proposer_tick(ctx: &SweepContext<'_>, toggle_on: bool) {
    let now = Utc::now();
    match proposer_pass(ctx, toggle_on).await {
        Ok(counts) => memory_workers::record_ran(LESSON_PROPOSER, Utc::now(), &counts),
        Err(reason) => memory_workers::record_skip(LESSON_PROPOSER, now, reason),
    }
}

async fn proposer_pass(ctx: &SweepContext<'_>, toggle_on: bool) -> Result<Value, Skip> {
    if !toggle_on {
        return Err(Skip::Off);
    }
    let every = Duration::from_secs(super::SWEEP_INTERVAL.as_secs() * PROPOSER_EVERY_TICKS);
    if !ctx
        .persist
        .holds_llm_role(LlmRole::LessonProposer, every)
        .await
    {
        return Err(Skip::NotLeader);
    }
    let settings = ProposerSettings::load(ctx.persist).await;
    let Some(llm) = proposer_client(ctx, &settings) else {
        log_no_key();
        return Err(Skip::NoClient);
    };
    let semantic = semantic_config(ctx.persist).await;
    let deps = ProposerDeps {
        persist: ctx.persist,
        llm: &llm,
        embedding: ctx.embedding,
        semantic: &semantic,
        settings: &settings,
    };
    memory_workers::mark_running(LESSON_PROPOSER, Utc::now());
    let pass = lesson_proposer::full_pass::run_full_pass(&deps).await;
    Ok(pass.counts())
}

fn log_no_key() {
    tracing::debug!(
        "quiescence_worker: the lesson proposer model runs on OpenRouter and no OpenRouter key is configured; lesson proposer skipped"
    );
}

#[cfg(test)]
mod tests {
    use arc_swap::ArcSwap;
    use serde_json::json;

    use super::*;

    fn empty_slot() -> OpenRouterKeySlot {
        Arc::new(ArcSwap::from_pointee(None))
    }

    fn effective_model_from(rows: &[(&str, serde_json::Value)]) -> String {
        let rows: Vec<(String, serde_json::Value)> = rows
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect();
        ProposerSettings::from_rows(&rows).effective_model
    }

    #[test]
    fn proposer_without_a_key_runs_on_a_codex_or_claude_model() {
        for model in ["codex/gpt-6-luna", "claude/haiku"] {
            let stored = effective_model_from(&[("judge_lessons.model", json!(model))]);
            let client = lesson_client(&SteererConfig::default(), &stored, None, &empty_slot());
            assert_eq!(client.map(|c| c.model().to_owned()).as_deref(), Some(model));
        }
    }

    #[test]
    fn proposer_without_a_key_skips_the_default_openrouter_model() {
        let stored = effective_model_from(&[]);
        let client = lesson_client(&SteererConfig::default(), &stored, None, &empty_slot());
        assert!(client.is_none());
    }

    use super::is_proposer_tick;

    #[test]
    fn proposer_runs_every_fifth_tick() {
        assert!(!is_proposer_tick(0));
        assert!(!is_proposer_tick(4));
        assert!(is_proposer_tick(5));
        assert!(is_proposer_tick(10));
    }
}
