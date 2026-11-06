use serde_json::json;

use super::models::check_routes_to;
use super::*;

pub(crate) struct InstalledProvider;

impl InstalledProvider {
    pub(crate) fn with(provider: &str) -> Self {
        let mut policy = ProviderPolicy::default();
        policy.set_provider(provider);
        let overlay = SettingsOverlay {
            llm_provider: Arc::new(policy),
            ..SettingsOverlay::default()
        };
        install(Arc::new(ArcSwap::from_pointee(overlay)));
        Self
    }
}

impl Drop for InstalledProvider {
    fn drop(&mut self) {
        install(Arc::new(ArcSwap::from_pointee(SettingsOverlay::default())));
    }
}

fn policy_for(provider: &str) -> ProviderPolicy {
    let mut policy = ProviderPolicy::default();
    assert!(policy.set_provider(provider));
    policy
}

#[test]
fn default_policy_is_per_role_and_keeps_every_roles_own_model() {
    let policy = ProviderPolicy::default();
    assert_eq!(policy.provider(), LlmProvider::PerRole);
    for tier in ModelTier::ALL {
        assert_eq!(policy.model_for(tier), None);
        assert_eq!(policy.resolve(Some(tier), "google/own"), "google/own");
    }
    assert_eq!(policy.resolve(None, "google/own"), "google/own");
}

#[test]
fn claude_policy_resolves_each_tier_to_its_claude_model() {
    let policy = policy_for("claude");
    assert_eq!(
        policy.resolve(Some(ModelTier::Fast), "google/own"),
        "claude/haiku"
    );
    assert_eq!(
        policy.resolve(Some(ModelTier::Standard), "google/own"),
        "claude/sonnet"
    );
    assert_eq!(
        policy.resolve(Some(ModelTier::Strong), "google/own"),
        "claude/opus"
    );
}

#[test]
fn provider_policy_leaves_a_client_without_a_tier_on_its_own_model() {
    let policy = policy_for("codex");
    assert_eq!(policy.resolve(None, "google/own"), "google/own");
}

#[test]
fn openrouter_policy_replaces_a_codex_own_model_with_the_openrouter_tier_model() {
    let policy = policy_for("openrouter");
    assert_eq!(
        policy.resolve(Some(ModelTier::Standard), "codex/gpt-6-sol"),
        "google/gemini-3.8-flash"
    );
}

#[test]
fn set_model_replaces_one_tier_of_one_provider_only() {
    let mut policy = policy_for("claude");
    assert!(policy.set_model(
        "llm.provider_models.claude.fast",
        " claude/claude-haiku-5-5 "
    ));
    assert_eq!(
        policy.model_for(ModelTier::Fast),
        Some("claude/claude-haiku-5-5")
    );
    assert_eq!(policy.model_for(ModelTier::Standard), Some("claude/sonnet"));
    assert_eq!(
        policy
            .tiers(LlmProvider::Codex)
            .map(|m| m.get(ModelTier::Fast)),
        Some("codex/gpt-6-luna")
    );
}

#[test]
fn set_model_rejects_blank_values_and_unknown_keys() {
    let mut policy = ProviderPolicy::default();
    assert!(!policy.set_model("llm.provider_models.claude.fast", "   "));
    assert!(!policy.set_model("llm.provider_models.claude.huge", "claude/opus"));
    assert!(!policy.set_provider("bedrock"));
}

#[test]
fn llm_provider_names_a_concrete_provider_except_per_role() {
    assert_eq!(LlmProvider::PerRole.concrete(), None);
    assert_eq!(
        LlmProvider::OpenRouter.concrete(),
        Some(Provider::OpenRouter)
    );
    assert_eq!(LlmProvider::Claude.concrete(), Some(Provider::Claude));
    assert_eq!(LlmProvider::Codex.concrete(), Some(Provider::Codex));
}

#[test]
fn model_settings_accept_every_provider_form_and_blank() {
    for key in MODEL_SETTING_KEYS {
        for ok in [
            "claude/sonnet",
            "codex/gpt-6-sol",
            "openrouter/openai/gpt-6-sol",
            "google/gemini-3.8-flash",
            "",
        ] {
            assert_eq!(validate_setting(key, &json!(ok)), Ok(()), "{key} {ok:?}");
        }
    }
}

#[test]
fn model_settings_reject_a_provider_prefix_with_no_model() {
    for key in MODEL_SETTING_KEYS {
        for bad in ["claude/", "codex/", "openrouter/"] {
            let result = validate_setting(key, &json!(bad));
            assert!(
                result.is_err_and(|e| e.contains("followed by a model name")),
                "{key} {bad}"
            );
        }
    }
}

#[test]
fn openrouter_slots_accept_explicit_openrouter_names_for_claude_and_gpt_models() {
    for model in [
        "openrouter/anthropic/claude-sonnet-5-5",
        "openrouter/openai/gpt-6-sol",
    ] {
        assert_eq!(
            validate_setting("llm.provider_models.openrouter.strong", &json!(model)),
            Ok(()),
            "{model}"
        );
    }
}

#[test]
fn cli_provider_slots_reject_openrouter_names() {
    let result = validate_setting(
        "llm.provider_models.claude.fast",
        &json!("openrouter/anthropic/claude-haiku"),
    );
    assert!(result.is_err_and(|e| e.contains("must start with `claude/`")));
}

#[test]
fn worker_roles_map_to_tiers_and_evaluation_has_none() {
    assert_eq!(
        ModelTier::for_role(LlmRole::SessionJudge),
        Some(ModelTier::Fast)
    );
    assert_eq!(
        ModelTier::for_role(LlmRole::LessonProposer),
        Some(ModelTier::Fast)
    );
    assert_eq!(
        ModelTier::for_role(LlmRole::JudgeWorker),
        Some(ModelTier::Standard)
    );
    assert_eq!(
        ModelTier::for_role(LlmRole::Steerer),
        Some(ModelTier::Strong)
    );
    assert_eq!(
        ModelTier::for_role(LlmRole::LessonRelations),
        Some(ModelTier::Strong)
    );
    assert_eq!(ModelTier::for_role(LlmRole::Eval), None);
    assert_eq!(ModelTier::for_role(LlmRole::Unattributed), None);
}

#[test]
fn every_provider_has_distinct_fast_and_strong_defaults_for_lesson_relations() {
    for provider in ["openrouter", "claude", "codex"] {
        let policy = policy_for(provider);
        assert_ne!(
            policy.model_for(ModelTier::Fast),
            policy.model_for(ModelTier::Strong),
            "{provider}"
        );
    }
}

#[test]
fn every_default_model_routes_to_its_own_provider() {
    for slot in SLOTS {
        assert_eq!(
            check_routes_to(slot.provider, slot.default),
            Ok(()),
            "{}",
            slot.key
        );
    }
}

#[test]
fn provider_variants_round_trip_through_parse() {
    for name in LlmProvider::VARIANTS {
        assert_eq!(
            LlmProvider::parse(name).map(LlmProvider::as_str),
            Some(*name)
        );
    }
}

#[test]
fn validate_setting_accepts_matching_prefixes_and_ignores_other_keys() {
    assert_eq!(
        validate_setting("llm.provider_models.claude.fast", &json!("claude/haiku")),
        Ok(())
    );
    assert_eq!(
        validate_setting(
            "llm.provider_models.codex.strong",
            &json!("codex/gpt-6.1-sol")
        ),
        Ok(())
    );
    assert_eq!(
        validate_setting(
            "llm.provider_models.openrouter.fast",
            &json!("google/gemini-3.5-flash-lite")
        ),
        Ok(())
    );
    assert_eq!(validate_setting("judge_worker.model", &json!("")), Ok(()));
}

#[test]
fn validate_setting_rejects_blank_and_wrong_provider_models() {
    let blank = validate_setting("llm.provider_models.claude.fast", &json!("  "));
    assert!(blank.is_err_and(|e| e.contains("cannot be blank")));
    let wrong = validate_setting("llm.provider_models.claude.fast", &json!("google/gemini"));
    assert!(wrong.is_err_and(|e| e.contains("must start with `claude/`")));
    let bare = validate_setting("llm.provider_models.codex.fast", &json!("codex/"));
    assert!(bare.is_err());
    let foreign = validate_setting(
        "llm.provider_models.openrouter.fast",
        &json!("claude/haiku"),
    );
    assert!(foreign.is_err_and(|e| e.contains("must be an OpenRouter model")));
}

#[test]
#[serial_test::serial(llm_provider)]
fn current_follows_the_installed_overlay_and_defaults_to_per_role() {
    install(Arc::new(ArcSwap::from_pointee(SettingsOverlay::default())));
    assert_eq!(current().provider(), LlmProvider::PerRole);
    {
        let _installed = InstalledProvider::with("codex");
        assert_eq!(current().provider(), LlmProvider::Codex);
        assert_eq!(resolve(Some(ModelTier::Fast), "x/y"), "codex/gpt-6-luna");
    }
    assert_eq!(current().provider(), LlmProvider::PerRole);
}

#[test]
#[serial_test::serial(llm_provider)]
fn current_sees_a_provider_change_stored_into_the_installed_overlay() {
    let shared = Arc::new(ArcSwap::from_pointee(SettingsOverlay::default()));
    install(shared.clone());
    assert_eq!(resolve(Some(ModelTier::Strong), "own"), "own");
    let mut policy = ProviderPolicy::default();
    policy.set_provider("claude");
    shared.store(Arc::new(SettingsOverlay {
        llm_provider: Arc::new(policy),
        ..SettingsOverlay::default()
    }));
    assert_eq!(resolve(Some(ModelTier::Strong), "own"), "claude/opus");
    install(Arc::new(ArcSwap::from_pointee(SettingsOverlay::default())));
}
