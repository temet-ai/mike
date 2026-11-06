use super::turn_sweep::resolve_turn_judge_runtime;

#[test]
fn turn_judge_settings_row_outranks_config_and_compiled_default() {
    let overlay = crate::settings::SettingsOverlay {
        judge_worker_model: Some("google/gemini-3.8-flash".to_owned()),
        judge_worker_system_prompt: Some("score it out of five".to_owned()),
        judge_worker_batch_size: Some(7),
        judge_worker_confidence_threshold: Some(0.9),
        ..Default::default()
    };

    let cfg = crate::steerer::config::JudgeConfig {
        model: Some("google/gemini-3.5-flash-lite".to_owned()),
        system_prompt: "config rubric".to_owned(),
        ..Default::default()
    };

    let r = resolve_turn_judge_runtime(&overlay, &cfg, "steerer/model");
    assert_eq!(r.model, "google/gemini-3.8-flash");
    assert_eq!(r.system_prompt, "score it out of five");
    assert_eq!(r.batch_size, 7);
    assert!((r.confidence_threshold - 0.9).abs() < f32::EPSILON);
}

#[test]
fn turn_judge_falls_back_config_then_compiled() {
    let overlay = crate::settings::SettingsOverlay::default();
    let cfg = crate::steerer::config::JudgeConfig {
        model: None,
        system_prompt: "config rubric".to_owned(),
        ..Default::default()
    };

    let r = resolve_turn_judge_runtime(&overlay, &cfg, "steerer/model");
    assert_eq!(r.model, "steerer/model");
    assert_eq!(r.system_prompt, "config rubric");

    let bare = crate::steerer::config::JudgeConfig::default();
    let r = resolve_turn_judge_runtime(&overlay, &bare, "steerer/model");
    assert_eq!(
        r.system_prompt,
        crate::analysis::judge::judge_system_prompt(),
        "an empty prompt at every layer must reach the compiled rubric"
    );
    assert_eq!(r.batch_size, bare.batch_size);
}

#[test]
fn turn_judge_blank_prompt_row_does_not_blank_the_rubric() {
    let overlay = crate::settings::SettingsOverlay {
        judge_worker_system_prompt: Some("   \n  ".to_owned()),
        ..Default::default()
    };
    let cfg = crate::steerer::config::JudgeConfig::default();

    let r = resolve_turn_judge_runtime(&overlay, &cfg, "steerer/model");
    assert_eq!(
        r.system_prompt,
        crate::analysis::judge::judge_system_prompt()
    );
}

fn keyless_slot() -> crate::llm::key::OpenRouterKeySlot {
    std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(None))
}

fn turn_judge_base() -> anyhow::Result<crate::llm::OpenRouterClient> {
    Ok(
        crate::llm::OpenRouterClient::new(String::new(), "steerer/model".to_owned(), None, None)?
            .with_attribution(crate::llm::LlmRole::JudgeWorker, None),
    )
}

#[test]
fn turn_judge_without_a_key_runs_on_a_codex_or_claude_model_from_settings() -> anyhow::Result<()> {
    let cfg = crate::steerer::config::JudgeConfig::default();
    for model in ["codex/gpt-6-sol", "claude/sonnet"] {
        let overlay = crate::settings::SettingsOverlay {
            judge_worker_model: Some(model.to_owned()),
            ..Default::default()
        };
        let runtime = resolve_turn_judge_runtime(&overlay, &cfg, "steerer/model");
        let llm = crate::llm::key::effective_client_for(
            &turn_judge_base()?,
            &keyless_slot(),
            &runtime.model,
        );
        assert_eq!(llm.map(|c| c.model().to_owned()).as_deref(), Some(model));
    }
    Ok(())
}

#[test]
fn turn_judge_without_a_key_skips_an_openrouter_model() -> anyhow::Result<()> {
    let cfg = crate::steerer::config::JudgeConfig::default();
    let overlay = crate::settings::SettingsOverlay::default();
    let runtime = resolve_turn_judge_runtime(&overlay, &cfg, "steerer/model");
    let llm =
        crate::llm::key::effective_client_for(&turn_judge_base()?, &keyless_slot(), &runtime.model);
    assert!(llm.is_none());
    Ok(())
}

#[test]
fn turn_judge_model_follows_the_provider_tier_so_a_keyless_codex_provider_runs()
-> anyhow::Result<()> {
    let mut policy = crate::llm::provider::ProviderPolicy::default();
    assert!(policy.set_provider("codex"));
    let overlay = crate::settings::SettingsOverlay {
        llm_provider: std::sync::Arc::new(policy),
        ..Default::default()
    };
    let cfg = crate::steerer::config::JudgeConfig::default();
    let runtime = resolve_turn_judge_runtime(&overlay, &cfg, "google/gemini-3.8-flash");
    assert_eq!(runtime.model, "codex/gpt-6-sol");
    let llm =
        crate::llm::key::effective_client_for(&turn_judge_base()?, &keyless_slot(), &runtime.model);
    assert!(llm.is_some());
    Ok(())
}
