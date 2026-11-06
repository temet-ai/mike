use std::sync::Arc;

use arc_swap::ArcSwap;

use super::SettingsOverlay;
use crate::llm::{LiveSettingsSource, LlmRuntimeSettings};

pub(crate) type SharedSettingsOverlay = Arc<ArcSwap<SettingsOverlay>>;

fn effort_or_shared(role_effort: &str, shared_effort: &str) -> String {
    match role_effort {
        "inherit" | "" => shared_effort.to_owned(),
        other => other.to_owned(),
    }
}

fn max_tokens_or_shared(role_max_tokens: i64, shared_max_tokens: i64) -> i64 {
    if role_max_tokens > 0 {
        role_max_tokens
    } else {
        shared_max_tokens
    }
}

fn build_settings(
    overlay: &SettingsOverlay,
    reasoning_effort: String,
    max_output_tokens: i64,
) -> LlmRuntimeSettings {
    LlmRuntimeSettings {
        reasoning_effort,
        max_output_tokens: u32::try_from(max_output_tokens).unwrap_or(0),
        spend_alert_fraction: overlay.llm_spend_alert_fraction,
        record_usage: overlay.llm_record_usage,
    }
}

pub(crate) fn resolve_judge_worker_llm_settings(overlay: &SettingsOverlay) -> LlmRuntimeSettings {
    build_settings(
        overlay,
        effort_or_shared(
            &overlay.judge_worker_reasoning_effort,
            &overlay.llm_reasoning_effort,
        ),
        max_tokens_or_shared(
            overlay.judge_worker_max_output_tokens,
            overlay.llm_max_output_tokens,
        ),
    )
}

pub(crate) fn resolve_session_judge_llm_settings(overlay: &SettingsOverlay) -> LlmRuntimeSettings {
    build_settings(
        overlay,
        effort_or_shared(
            &overlay.session_judge_reasoning_effort,
            &overlay.llm_reasoning_effort,
        ),
        max_tokens_or_shared(
            overlay.session_judge_max_output_tokens,
            overlay.llm_max_output_tokens,
        ),
    )
}

pub(crate) fn resolve_steerer_llm_settings(overlay: &SettingsOverlay) -> LlmRuntimeSettings {
    build_settings(
        overlay,
        effort_or_shared(
            &overlay.steerer_reasoning_effort,
            &overlay.llm_reasoning_effort,
        ),
        max_tokens_or_shared(
            overlay.steerer_max_output_tokens,
            overlay.llm_max_output_tokens,
        ),
    )
}

pub(crate) fn resolve_shared_llm_settings(overlay: &SettingsOverlay) -> LlmRuntimeSettings {
    build_settings(
        overlay,
        overlay.llm_reasoning_effort.clone(),
        overlay.llm_max_output_tokens,
    )
}

fn live_source(
    overlay: &SharedSettingsOverlay,
    resolve: fn(&SettingsOverlay) -> LlmRuntimeSettings,
) -> LiveSettingsSource {
    let overlay = overlay.clone();
    Arc::new(move || resolve(&overlay.load()))
}

pub(crate) fn session_judge_live_source(overlay: &SharedSettingsOverlay) -> LiveSettingsSource {
    live_source(overlay, resolve_session_judge_llm_settings)
}

pub(crate) fn steerer_live_source(overlay: &SharedSettingsOverlay) -> LiveSettingsSource {
    live_source(overlay, resolve_steerer_llm_settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::OpenRouterClient;

    fn shared(overlay: SettingsOverlay) -> SharedSettingsOverlay {
        Arc::new(ArcSwap::from_pointee(overlay))
    }

    fn client() -> anyhow::Result<OpenRouterClient> {
        OpenRouterClient::new(
            "sk-test".to_owned(),
            "google/gemini-3.5-flash-lite".to_owned(),
            None,
            None,
        )
        .map_err(Into::into)
    }

    #[test]
    fn session_judge_client_carries_the_overlay_reasoning_effort() -> anyhow::Result<()> {
        let overlay = shared(SettingsOverlay {
            session_judge_reasoning_effort: "high".to_owned(),
            session_judge_max_output_tokens: 2000,
            ..SettingsOverlay::default()
        });

        let llm = client()?.with_live_settings(session_judge_live_source(&overlay));

        let effective = llm.effective_settings();
        assert_eq!(
            effective.as_ref().map(|s| s.reasoning_effort.clone()),
            Some("high".to_owned())
        );
        assert_eq!(effective.as_ref().map(|s| s.max_output_tokens), Some(2000));
        Ok(())
    }

    #[test]
    fn live_source_follows_overlay_changes_after_the_client_is_built() -> anyhow::Result<()> {
        let overlay = shared(SettingsOverlay::default());
        let llm = client()?.with_live_settings(steerer_live_source(&overlay));
        assert_eq!(
            llm.effective_settings().map(|s| s.reasoning_effort),
            Some("none".to_owned())
        );

        overlay.store(Arc::new(SettingsOverlay {
            steerer_reasoning_effort: "high".to_owned(),
            ..SettingsOverlay::default()
        }));

        assert_eq!(
            llm.effective_settings().map(|s| s.reasoning_effort),
            Some("high".to_owned())
        );
        Ok(())
    }

    #[test]
    fn client_without_a_source_has_no_effective_settings() -> anyhow::Result<()> {
        assert!(client()?.effective_settings().is_none());
        Ok(())
    }

    #[test]
    fn inherit_falls_back_to_the_shared_reasoning_effort_and_tokens() {
        let overlay = SettingsOverlay {
            llm_reasoning_effort: "medium".to_owned(),
            llm_max_output_tokens: 1200,
            steerer_reasoning_effort: "inherit".to_owned(),
            steerer_max_output_tokens: 0,
            ..SettingsOverlay::default()
        };

        let settings = resolve_steerer_llm_settings(&overlay);

        assert_eq!(settings.reasoning_effort, "medium");
        assert_eq!(settings.max_output_tokens, 1200);
    }

    #[test]
    fn role_overrides_win_over_the_shared_values() {
        let overlay = SettingsOverlay {
            llm_reasoning_effort: "none".to_owned(),
            llm_max_output_tokens: 800,
            session_judge_reasoning_effort: "high".to_owned(),
            session_judge_max_output_tokens: 2000,
            ..SettingsOverlay::default()
        };

        let settings = resolve_session_judge_llm_settings(&overlay);

        assert_eq!(settings.reasoning_effort, "high");
        assert_eq!(settings.max_output_tokens, 2000);
    }

    #[test]
    fn shared_settings_ignore_every_role_override() {
        let overlay = SettingsOverlay {
            llm_reasoning_effort: "low".to_owned(),
            steerer_reasoning_effort: "high".to_owned(),
            session_judge_reasoning_effort: "high".to_owned(),
            judge_worker_reasoning_effort: "high".to_owned(),
            ..SettingsOverlay::default()
        };

        assert_eq!(
            resolve_shared_llm_settings(&overlay).reasoning_effort,
            "low"
        );
    }

    #[test]
    fn turn_judge_default_reasoning_effort_is_low() {
        let settings = resolve_judge_worker_llm_settings(&SettingsOverlay::default());

        assert_eq!(settings.reasoning_effort, "low");
    }
}
