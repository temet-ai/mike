use super::*;

#[test]
fn settings_meta_keys_are_unique() {
    let mut keys: Vec<&str> = SETTINGS_META.iter().map(|m| m.key).collect();
    let original_len = keys.len();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(
        keys.len(),
        original_len,
        "SETTINGS_META must have unique keys"
    );
}

#[test]
fn settings_meta_enum_variants_non_empty() {
    for meta in SETTINGS_META.iter() {
        if meta.kind == SettingKind::Enum {
            let variants = meta.enum_variants.unwrap_or(&[]);
            assert!(
                !variants.is_empty(),
                "setting '{}' of Enum kind must have non-empty enum_variants",
                meta.key
            );
        }
    }
}

#[test]
fn settings_overlay_defaults_match_toggle_off_by_default_contract() {
    let overlay = SettingsOverlay::default();
    assert!(
        !overlay.reminders_inject_lessons,
        "reminders.inject_lessons must default false"
    );
    assert!(
        !overlay.compression_dedup_enabled,
        "compression_dedup.enabled must default false"
    );
    assert!(
        !overlay.file_read_compression_protected_paths.is_empty(),
        "file_read_compression.protected_paths must not default empty"
    );
}

#[test]
fn weasel_correction_prompt_default_matches_compiled_and_catalogue() {
    let overlay_default = SettingsOverlay::default().weasel_intercept_correction_prompt;
    let compiled_default =
        crate::weasel_intercept::config::WeaselInterceptConfig::default().correction_prompt;
    assert_eq!(
        overlay_default, compiled_default,
        "SettingsOverlay::default must mirror WeaselInterceptConfig::default"
    );

    let Some(meta) = SETTINGS_META
        .iter()
        .find(|m| m.key == "weasel_intercept.correction_prompt")
    else {
        panic!("weasel_intercept.correction_prompt must be registered in SETTINGS_META");
    };
    let Some(meta_default) = meta.default.as_str() else {
        panic!("weasel_intercept.correction_prompt default must be a string");
    };
    assert_eq!(
        overlay_default, meta_default,
        "SETTINGS_META's declared default must mirror the compiled default"
    );
}

#[test]
fn settings_meta_text_kind_defaults_are_never_blank() {
    for meta in SETTINGS_META.iter() {
        if meta.kind != SettingKind::Text {
            continue;
        }
        let default = meta.default.as_str().unwrap_or_else(|| {
            panic!(
                "setting '{}' of Text kind must have a string default",
                meta.key
            )
        });
        assert!(
            !default.trim().is_empty(),
            "setting '{}' of Text kind must not default to a blank prompt",
            meta.key
        );
    }
}

#[test]
fn settings_overlay_default_matches_file_read_compression_compiled_defaults() {
    let overlay = SettingsOverlay::default();
    assert_eq!(overlay.file_read_compression_min_bytes, 1500);
    assert_eq!(overlay.file_read_compression_head_lines, 40);
    assert_eq!(overlay.file_read_compression_tail_lines, 20);
    assert_eq!(
        overlay.file_read_compression_enabled_producers,
        vec!["Read".to_owned()]
    );
    assert_eq!(
        overlay.file_read_compression_protected_paths,
        vec![
            "**/CLAUDE.md".to_owned(),
            "**/BRIEF*.md".to_owned(),
            "**/.claude/**".to_owned(),
            "**/docs/plans/**".to_owned(),
            "**/artifacts/threads/**".to_owned(),
            "**/*.rules.md".to_owned(),
        ]
    );
}

#[test]
fn settings_meta_declares_no_secret_shaped_key() {
    for meta in SETTINGS_META.iter() {
        assert!(
            !service::is_secret_setting_key(meta.key),
            "setting '{}' looks like a secret; secrets live in the environment and only a `_env` variable name may be a setting",
            meta.key
        );
    }
}

fn meta_default_string(key: &str) -> String {
    let Some(meta) = SETTINGS_META.iter().find(|m| m.key == key) else {
        panic!("{key} must be registered in SETTINGS_META");
    };
    let Some(value) = meta.default.as_str() else {
        panic!("{key} default must be a string");
    };
    value.to_owned()
}

#[test]
fn model_settings_registry_defaults_match_compiled_defaults() {
    let steerer = crate::steerer::config::SteererConfig::default();
    let lesson_relations = crate::config::lesson_relations::LessonRelationsConfig::default();
    let session_judge = crate::config::session_judge::SessionJudgeConfig::default();

    assert_eq!(meta_default_string("steerer.model"), steerer.model);
    assert_eq!(
        Some(meta_default_string("judge_worker.model")),
        steerer.judge.model
    );
    assert_eq!(
        meta_default_string("session_judge.model"),
        session_judge.model
    );
    assert_eq!(
        meta_default_string("lesson_relations.proposer_model"),
        lesson_relations.proposer_model
    );
    assert_eq!(
        meta_default_string("lesson_relations.adjudicator_model"),
        lesson_relations.adjudicator_model
    );
}

#[test]
fn judge_worker_reasoning_effort_registry_default_matches_overlay_default() {
    assert_eq!(
        meta_default_string("judge_worker.reasoning_effort"),
        SettingsOverlay::default().judge_worker_reasoning_effort
    );
}
