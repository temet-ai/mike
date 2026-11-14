use serde_json::json;

use super::{apply_stored, resolve_models};
use crate::config::ProxyConfig;
use crate::lessons_settings::LessonRelationsConfig;

fn rows(pairs: &[(&str, serde_json::Value)]) -> Vec<(String, serde_json::Value)> {
    crate::feature_settings::install();
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

#[test]
fn apply_stored_takes_the_stored_enabled_flag_over_the_boot_config() {
    let mut config = ProxyConfig::default();
    crate::feature_settings::install();
    config
        .sections
        .update::<LessonRelationsConfig>(|relations| relations.enabled = true);
    apply_stored(
        &mut config,
        rows(&[("lesson_relations.enabled", json!(false))]),
    );
    assert!(!config.sections.get::<LessonRelationsConfig>().enabled);
}

#[test]
fn resolve_models_uses_the_stored_proposer_and_adjudicator() -> anyhow::Result<()> {
    let mut config = ProxyConfig::default();
    let overlay = apply_stored(
        &mut config,
        rows(&[
            ("lesson_relations.proposer_model", json!("claude/opus")),
            ("lesson_relations.adjudicator_model", json!("claude/haiku")),
        ]),
    );
    let (proposer, adjudicator) = resolve_models(&config, &overlay.llm_provider)?;
    assert_eq!(proposer, "claude/opus");
    assert_eq!(adjudicator, "claude/haiku");
    Ok(())
}

#[test]
fn resolve_models_refuses_identical_stored_models_and_names_the_settings() -> anyhow::Result<()> {
    let mut config = ProxyConfig::default();
    let overlay = apply_stored(
        &mut config,
        rows(&[
            ("lesson_relations.proposer_model", json!("claude/sonnet")),
            ("lesson_relations.adjudicator_model", json!("claude/sonnet")),
        ]),
    );
    let Err(error) = resolve_models(&config, &overlay.llm_provider) else {
        anyhow::bail!("identical models must be refused");
    };
    let message = error.to_string();
    assert!(
        message.contains("lesson_relations.proposer_model"),
        "{message}"
    );
    assert!(
        message.contains("lesson_relations.adjudicator_model"),
        "{message}"
    );
    assert!(message.contains("claude/sonnet"), "{message}");
    Ok(())
}

#[test]
fn resolve_models_keeps_boot_models_when_nothing_is_stored() -> anyhow::Result<()> {
    let mut config = ProxyConfig::default();
    let overlay = apply_stored(&mut config, Vec::new());
    let (proposer, adjudicator) = resolve_models(&config, &overlay.llm_provider)?;
    assert_eq!(
        proposer,
        config
            .sections
            .get::<LessonRelationsConfig>()
            .proposer_model
    );
    assert_eq!(
        adjudicator,
        config
            .sections
            .get::<LessonRelationsConfig>()
            .adjudicator_model
    );
    Ok(())
}
