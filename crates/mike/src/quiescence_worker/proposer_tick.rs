use std::sync::Arc;
use std::time::Duration;

use crate::lesson_proposer::{self, ProposerDeps, ProposerSettings};
use crate::llm::key::OpenRouterKeySlot;
use crate::llm::{LlmRole, LlmUsageSink, OpenRouterClient};
use crate::semantic::SemanticConfig;
use crate::steerer::config::SteererConfig;

use super::sweep::SweepContext;

pub(super) const PROPOSER_EVERY_TICKS: u64 = 5;

pub(super) fn is_proposer_tick(tick: u64) -> bool {
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

fn lesson_client(
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

async fn semantic_config(ctx: &SweepContext<'_>) -> SemanticConfig {
    ctx.persist
        .fetch_settings()
        .await
        .map(|rows| SemanticConfig::from_rows(&rows))
        .unwrap_or_default()
}

pub(super) async fn run_proposer_tick(ctx: &SweepContext<'_>, toggle_on: bool) {
    if !toggle_on {
        return;
    }
    let every = Duration::from_secs(super::SWEEP_INTERVAL.as_secs() * PROPOSER_EVERY_TICKS);
    if !ctx
        .persist
        .holds_llm_role(LlmRole::LessonProposer, every)
        .await
    {
        return;
    }
    let settings = ProposerSettings::load(ctx.persist).await;
    let Some(llm) = proposer_client(ctx, &settings) else {
        log_no_key();
        return;
    };
    let semantic = semantic_config(ctx).await;
    let deps = ProposerDeps {
        persist: ctx.persist,
        llm: &llm,
        embedding: ctx.embedding,
        semantic: &semantic,
        settings: &settings,
    };
    let report = lesson_proposer::run_tick(&deps).await;
    tracing::debug!(?report, "quiescence_worker: lesson proposer pass");
    run_topic_pass(ctx, &llm).await;
}

async fn run_topic_pass(ctx: &SweepContext<'_>, llm: &OpenRouterClient) {
    let topics =
        lesson_proposer::topics::run_pass(ctx.persist, llm, lesson_proposer::topics::BATCH).await;
    tracing::debug!(?topics, "quiescence_worker: lesson topic pass");
    run_skill_author_pass(ctx, llm).await;
}

async fn run_skill_author_pass(ctx: &SweepContext<'_>, llm: &OpenRouterClient) {
    let author = lesson_proposer::skill_author::run_pass(ctx.persist, llm).await;
    tracing::debug!(?author, "quiescence_worker: skill author pass");
    lesson_proposer::skill_eval::run_pass(ctx.persist, llm).await;
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
