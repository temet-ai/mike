use serde_json::json;

use super::{
    HarvesterConfig, LessonRelationsConfig, LessonWorkerConfig, RecallInjectorConfig, RecallOverlay,
};
use crate::config::ProxyConfig;

fn meta_default(key: &str) -> serde_json::Value {
    crate::settings::SETTINGS_META
        .iter()
        .find(|meta| meta.key == key)
        .map(|meta| meta.default.clone())
        .unwrap_or_else(|| panic!("{key} is not registered"))
}

#[test]
fn lessons_settings_defaults_keep_their_values() {
    let worker = LessonWorkerConfig::default();
    assert!(worker.enabled);
    assert_eq!(worker.max_lessons_per_project, 100);
    assert!(!RecallInjectorConfig::default().enabled);
    let harvester = HarvesterConfig::default();
    assert!(harvester.enabled, "harvester must default to enabled");
    assert_eq!(harvester.max_lesson_bytes, 4096);
    let relations = LessonRelationsConfig::default();
    assert!(!relations.enabled);
    assert_ne!(relations.proposer_model, relations.adjudicator_model);
}

#[test]
fn lessons_settings_registered_meta_matches_compiled_defaults() {
    super::register();
    let relations = LessonRelationsConfig::default();
    assert_eq!(
        meta_default("lesson_relations.proposer_model"),
        json!(relations.proposer_model)
    );
    assert_eq!(
        meta_default("lesson_relations.adjudicator_model"),
        json!(relations.adjudicator_model)
    );
    assert_eq!(
        meta_default("recall_injector.max_lessons_per_topic"),
        json!(3)
    );
    assert_eq!(meta_default("lesson_worker.gc_interval_secs"), json!(3600));
    assert!(
        crate::settings::SETTINGS_META
            .iter()
            .any(|meta| meta.key == super::meta_memory_loop::SUPPLY_VERDICT_PROMPT_KEY)
    );
}

#[test]
fn lessons_settings_toml_sections_load_into_typed_config() -> anyhow::Result<()> {
    super::register();
    let table: toml::Table = toml::from_str(
        "[lesson_worker]\ndedup_threshold = 0.5\n[harvester]\nenabled = false\n\
         [lesson_relations]\nenabled = true\n[recall_injector]\nmax_lessons_per_topic = 2\n",
    )?;
    crate::config::load::file::validate(&table)?;
    let config: ProxyConfig = figment::Figment::from(figment::providers::Serialized::defaults(
        ProxyConfig::default(),
    ))
    .merge(figment::providers::Serialized::defaults(table))
    .extract()?;
    assert_eq!(
        config.sections.get::<LessonWorkerConfig>().dedup_threshold,
        0.5
    );
    assert!(!config.sections.get::<HarvesterConfig>().enabled);
    assert!(config.sections.get::<LessonRelationsConfig>().enabled);
    assert_eq!(
        config
            .sections
            .get::<RecallInjectorConfig>()
            .max_lessons_per_topic,
        2
    );
    let toggles = crate::feature_toggles::build_feature_toggle_overlay(&config);
    assert!(!toggles.load().harvester);
    Ok(())
}

#[test]
fn lessons_settings_db_rows_fold_onto_config_and_overlay() {
    super::register();
    let rows = vec![
        (
            "lesson_worker.max_lessons_per_project".to_owned(),
            json!(42),
        ),
        ("lesson_relations.enabled".to_owned(), json!(true)),
        (
            "lesson_relations.candidate_distance_threshold".to_owned(),
            json!(0.4),
        ),
        ("recall_injector.max_lessons_per_topic".to_owned(), json!(6)),
    ];
    let mut config = ProxyConfig::default();
    crate::config::apply_settings_overrides(&mut config, &rows);
    assert_eq!(
        config
            .sections
            .get::<LessonWorkerConfig>()
            .max_lessons_per_project,
        42
    );
    let relations = config.sections.get::<LessonRelationsConfig>();
    assert!(relations.enabled);
    assert_eq!(relations.candidate_distance_threshold, 0.4);

    let overlay = crate::settings::apply_db_rows_to_overlay(rows);
    assert_eq!(
        overlay
            .features
            .get::<RecallOverlay>()
            .max_lessons_per_topic,
        6
    );
    let default_overlay = crate::settings::SettingsOverlay::default();
    assert_eq!(
        default_overlay
            .features
            .get::<RecallOverlay>()
            .max_lessons_per_topic,
        3
    );
}
