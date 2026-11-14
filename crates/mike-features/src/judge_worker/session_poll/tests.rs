use super::*;

#[test]
fn session_judge_settings_row_outranks_config() {
    let overlay = crate::settings::SettingsOverlay {
        session_judge_model: Some("openai/gpt-5.6-luna".to_owned()),
        session_judge_system_prompt: Some("two paragraphs, no more".to_owned()),
        session_judge_batch_size: Some(3),
        session_judge_min_session_age_secs: Some(900),
        session_judge_max_turns_in_prompt: Some(12),
        ..Default::default()
    };

    let cfg = crate::features_settings::sections::session_judge::SessionJudgeConfig::default();
    let r = resolve_session_judge_runtime(&overlay, &cfg);
    assert_eq!(r.model, "openai/gpt-5.6-luna");
    assert_eq!(r.system_prompt, "two paragraphs, no more");
    assert_eq!(r.batch_size, 3);
    assert_eq!(r.min_session_age_secs, 900);
    assert_eq!(r.max_turns_in_prompt, 12);
}

#[test]
fn session_judge_unset_settings_keep_the_compiled_prompt_and_model() {
    let overlay = crate::settings::SettingsOverlay::default();
    let cfg = crate::features_settings::sections::session_judge::SessionJudgeConfig::default();
    let r = resolve_session_judge_runtime(&overlay, &cfg);
    assert_eq!(r.model, cfg.model);
    assert_eq!(r.system_prompt, SESSION_JUDGE_SYSTEM_PROMPT);
    assert_eq!(r.batch_size, cfg.batch_size);
    assert_eq!(r.min_session_age_secs, cfg.min_session_age_secs);
    assert_eq!(r.max_turns_in_prompt, cfg.max_turns_in_prompt);
}

#[test]
fn session_judge_runtime_defaults_to_three_attempts_per_day() {
    let overlay = crate::settings::SettingsOverlay::default();
    let cfg = crate::features_settings::sections::session_judge::SessionJudgeConfig::default();
    assert_eq!(
        resolve_session_judge_runtime(&overlay, &cfg).max_attempts_per_day,
        3
    );
    let capped = crate::settings::SettingsOverlay {
        session_judge_max_attempts_per_day: Some(7),
        ..Default::default()
    };
    assert_eq!(
        resolve_session_judge_runtime(&capped, &cfg).max_attempts_per_day,
        7
    );
}

const SESSION_POLL_SOURCES: [(&str, &str); 4] = [
    ("cycle.rs", include_str!("cycle.rs")),
    ("verdict_write.rs", include_str!("verdict_write.rs")),
    ("mod.rs", include_str!("mod.rs")),
    ("digest.rs", include_str!("digest.rs")),
];

fn occurrences(needle: &str) -> Vec<&'static str> {
    SESSION_POLL_SOURCES
        .iter()
        .flat_map(|(name, text)| std::iter::repeat_n(*name, text.matches(needle).count()))
        .collect()
}

#[test]
fn every_session_judge_llm_call_records_an_attempt() -> anyhow::Result<()> {
    assert_eq!(
        occurrences(".verdict::<"),
        vec!["verdict_write.rs"],
        "the session judge sends its LLM call from call_judge_llm only"
    );
    assert_eq!(
        occurrences("call_judge_llm("),
        vec!["cycle.rs", "verdict_write.rs"],
        "call_judge_llm has one caller"
    );
    let cycle = include_str!("cycle.rs");
    let start = cycle
        .find("call_judge_llm(")
        .ok_or_else(|| anyhow::anyhow!("no call site"))?;
    let tail = &cycle[start..];
    let end = tail
        .find("\n}\n")
        .ok_or_else(|| anyhow::anyhow!("call site has no enclosing end"))?;
    let arms = &tail[..end];
    for arm in [
        "JudgeCall::BreakerTripped",
        "JudgeCall::Failed",
        "JudgeCall::Verdict",
    ] {
        let at = arms
            .find(arm)
            .ok_or_else(|| anyhow::anyhow!("{arm} is not handled"))?;
        let rest = &arms[at..];
        let next_arm = rest[arm.len()..]
            .find("JudgeCall::")
            .map_or(rest.len(), |i| i + arm.len());
        assert!(
            rest[..next_arm].contains("record_attempt("),
            "{arm} must record an attempt"
        );
    }
    let refused = arms
        .find("JudgeCall::Refused")
        .ok_or_else(|| anyhow::anyhow!("Refused is not handled"))?;
    let refused_arm = &arms[refused..];
    let refused_end = refused_arm["JudgeCall::Refused".len()..]
        .find("JudgeCall::")
        .unwrap_or(refused_arm.len());
    assert!(
        !refused_arm[..refused_end].contains("record_attempt("),
        "a refusal is not a call and records nothing"
    );
    Ok(())
}

fn keyless_slot() -> crate::llm::key::OpenRouterKeySlot {
    Arc::new(ArcSwap::from_pointee(None))
}

fn session_judge_base() -> anyhow::Result<OpenRouterClient> {
    let cfg = crate::features_settings::sections::session_judge::SessionJudgeConfig::default();
    Ok(
        OpenRouterClient::new(String::new(), cfg.model.clone(), None, None)?
            .with_attribution(LlmRole::SessionJudge, None),
    )
}

#[test]
fn session_judge_without_a_key_runs_on_a_claude_or_codex_model() -> anyhow::Result<()> {
    let cfg = crate::features_settings::sections::session_judge::SessionJudgeConfig::default();
    for model in ["claude/haiku", "codex/gpt-6-luna"] {
        let overlay = crate::settings::SettingsOverlay {
            session_judge_model: Some(model.to_owned()),
            ..Default::default()
        };
        let runtime = resolve_session_judge_runtime(&overlay, &cfg);
        let llm =
            cycle::resolve_effective_llm(&session_judge_base()?, &keyless_slot(), &runtime.model);
        assert_eq!(llm.map(|c| c.model().to_owned()).as_deref(), Some(model));
    }
    Ok(())
}

#[test]
fn session_judge_without_a_key_skips_an_openrouter_model() -> anyhow::Result<()> {
    let cfg = crate::features_settings::sections::session_judge::SessionJudgeConfig::default();
    let overlay = crate::settings::SettingsOverlay::default();
    let runtime = resolve_session_judge_runtime(&overlay, &cfg);
    let llm = cycle::resolve_effective_llm(&session_judge_base()?, &keyless_slot(), &runtime.model);
    assert!(llm.is_none());
    Ok(())
}
