use std::sync::Arc;
use std::time::Duration;

use crate::config::steerer::SteererConfig;
use crate::llm::key::OpenRouterKeySlot;
use crate::llm::{LlmRole, LlmUsageSink, OpenRouterClient};
use crate::settings::SharedSettingsOverlay;
use crate::steerer::guard::label::{LabelPass, run_pass};
use crate::steerer::reply_label::run_reply_label_pass;

use super::sweep::SweepContext;

pub(super) const LABEL_EVERY_TICKS: u64 = 5;

pub(super) fn is_label_tick(tick: u64) -> bool {
    tick > 0 && tick.is_multiple_of(LABEL_EVERY_TICKS)
}

fn eval_client(ctx: &SweepContext<'_>) -> Option<OpenRouterClient> {
    labeller_client(ctx.config, ctx.settings, ctx.usage_sink, ctx.key_slot)
}

fn labeller_client(
    config: &SteererConfig,
    settings: &SharedSettingsOverlay,
    usage_sink: Option<&Arc<dyn LlmUsageSink>>,
    key_slot: &OpenRouterKeySlot,
) -> Option<OpenRouterClient> {
    let model = settings.load().live_steerer_model(&config.model);
    let base = OpenRouterClient::new(
        String::new(),
        model,
        Some(config.openrouter_base_url.clone()),
        Some(Duration::from_secs(config.request_timeout_secs)),
    )
    .ok()?
    .with_attribution(LlmRole::Eval, usage_sink)
    .with_tier(crate::llm::provider::ModelTier::Strong);
    crate::llm::key::effective_client(&base, key_slot)
}

async fn run_guard_labels(ctx: &SweepContext<'_>, llm: &OpenRouterClient) {
    let report = run_pass(&LabelPass {
        persist: ctx.persist,
        llm,
        toggle_on: true,
    })
    .await;
    tracing::debug!(?report, "quiescence_worker: turn guard labeller pass");
}

async fn run_reply_labels(ctx: &SweepContext<'_>, llm: &OpenRouterClient) {
    let report = run_reply_label_pass(ctx.persist, llm).await;
    tracing::debug!(?report, "quiescence_worker: steering reply labeller pass");
}

pub(super) async fn run_label_tick(ctx: &SweepContext<'_>, toggle_on: bool) {
    let every = Duration::from_secs(super::SWEEP_INTERVAL.as_secs() * LABEL_EVERY_TICKS);
    if !ctx.persist.holds_llm_role(LlmRole::Eval, every).await {
        return;
    }
    let Some(llm) = eval_client(ctx) else {
        log_no_key();
        return;
    };
    if toggle_on {
        run_guard_labels(ctx, &llm).await;
    }
    run_reply_labels(ctx, &llm).await;
}

fn log_no_key() {
    tracing::debug!(
        "quiescence_worker: the steerer model runs on OpenRouter and no OpenRouter key is configured; turn guard labeller skipped"
    );
}

#[cfg(test)]
mod tests {
    use arc_swap::ArcSwap;

    use super::*;
    use crate::settings::SettingsOverlay;

    fn empty_slot() -> OpenRouterKeySlot {
        Arc::new(ArcSwap::from_pointee(None))
    }

    fn settings_with_steerer_model(model: Option<&str>) -> SharedSettingsOverlay {
        Arc::new(ArcSwap::from_pointee(SettingsOverlay {
            steerer_model: model.map(str::to_owned),
            ..SettingsOverlay::default()
        }))
    }

    fn config_on(model: &str) -> SteererConfig {
        SteererConfig {
            model: model.to_owned(),
            ..SteererConfig::default()
        }
    }

    #[test]
    fn labeller_without_a_key_runs_on_a_codex_or_claude_steerer_model() {
        for model in ["codex/gpt-6-sol", "claude/opus"] {
            let client = labeller_client(
                &config_on("google/gemini-3.8-flash"),
                &settings_with_steerer_model(Some(model)),
                None,
                &empty_slot(),
            );
            assert_eq!(client.map(|c| c.model().to_owned()).as_deref(), Some(model));
        }
    }

    #[test]
    fn labeller_without_a_key_skips_an_openrouter_steerer_model() {
        let client = labeller_client(
            &config_on("google/gemini-3.8-flash"),
            &settings_with_steerer_model(None),
            None,
            &empty_slot(),
        );
        assert!(client.is_none());
    }

    #[test]
    fn labeller_follows_the_live_steerer_model_over_the_boot_model() {
        let live_wins = labeller_client(
            &config_on("google/gemini-3.8-flash"),
            &settings_with_steerer_model(Some("codex/gpt-6-sol")),
            None,
            &empty_slot(),
        );
        assert!(live_wins.is_some());
        let boot_ignored = labeller_client(
            &config_on("codex/gpt-6-sol"),
            &settings_with_steerer_model(Some("google/gemini-3.8-flash")),
            None,
            &empty_slot(),
        );
        assert!(boot_ignored.is_none());
    }

    use super::is_label_tick;

    #[test]
    fn labeller_runs_every_fifth_tick() {
        assert!(!is_label_tick(0));
        assert!(!is_label_tick(4));
        assert!(is_label_tick(5));
        assert!(is_label_tick(10));
    }
}
