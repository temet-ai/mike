use super::*;

const EXPECTED_KEYS: &[&str] = &[
    "assistant.provider",
    "prompt_assistant.codex_model",
    "prompt_assistant.model",
    "prompt_assistant.effort",
    "prompt_assistant.instructions",
    "prompt_assistant.read_prompt",
    "prompt_assistant.read_edits",
    "prompt_assistant.read_project_instructions",
    "prompt_assistant.read_versions",
    "prompt_assistant.read_memory",
    "prompt_assistant.read_other_projects",
    "prompt_assistant.may_apply",
    "prompt_assistant.capture",
    "prompt_assistant.max_turns",
    "prompt_assistant.daily_budget_usd",
    "prompt_assistant.keep_days",
    "assistant.anthropic_auth_mode",
];

#[test]
fn entries_declares_every_spec_key_exactly_once() {
    let mut keys: Vec<&str> = entries().iter().map(|m| m.key).collect();
    keys.sort_unstable();
    let mut expected: Vec<&str> = EXPECTED_KEYS.to_vec();
    expected.sort_unstable();
    assert_eq!(keys, expected);
}

#[test]
fn entries_are_all_global_scope() {
    for meta in entries() {
        assert_eq!(
            meta.scope,
            SettingScope::Global,
            "'{}' must be Global — the spec table has no per-session prompt assistant \
             setting",
            meta.key
        );
    }
}

#[test]
fn read_scope_toggles_apply_on_next_message() {
    let read_keys = [
        "prompt_assistant.read_prompt",
        "prompt_assistant.read_edits",
        "prompt_assistant.read_project_instructions",
        "prompt_assistant.read_versions",
        "prompt_assistant.read_memory",
        "prompt_assistant.read_other_projects",
    ];
    for meta in entries() {
        if read_keys.contains(&meta.key) {
            assert_eq!(
                meta.applies,
                SettingApplies::NextMessage,
                "'{}' gates a tool list rebuilt fresh before every message, not once per \
                 conversation",
                meta.key
            );
        }
    }
}

#[test]
fn non_sweep_keys_apply_on_next_message() {
    for meta in entries() {
        if meta.key == "prompt_assistant.keep_days" {
            continue;
        }
        assert_eq!(
            meta.applies,
            SettingApplies::NextMessage,
            "'{}' is re-read fresh before every message the assistant sends — there is no \
             worker cycle for it to wait on",
            meta.key
        );
    }
}

#[test]
fn keep_days_applies_on_the_sweeps_next_cycle() {
    let owned = entries();
    let by_key: std::collections::HashMap<&str, &SettingMeta> =
        owned.iter().map(|m| (m.key, m)).collect();
    assert_eq!(
        by_key["prompt_assistant.keep_days"].applies,
        SettingApplies::NextCycle,
        "keep_days genuinely gates a recurring sweep cycle, unlike every other \
         prompt_assistant setting"
    );
}

#[test]
fn read_memory_description_names_the_claude_code_memory_path() {
    let owned = entries();
    let by_key: std::collections::HashMap<&str, &SettingMeta> =
        owned.iter().map(|m| (m.key, m)).collect();
    let description = by_key["prompt_assistant.read_memory"].description;
    assert!(
        description.contains("~/.claude/projects/"),
        "read_memory must describe the Claude Code memory path it actually reads, got: {description}"
    );
    assert!(
        !description.contains("lessons and notes"),
        "read_memory must not describe itself as reading recorded lessons — it reads \
         Claude Code memory files, got: {description}"
    );
}

#[test]
fn read_defaults_match_spec_table() {
    let defaults: std::collections::HashMap<&str, bool> = entries()
        .into_iter()
        .filter_map(|m| m.default.as_bool().map(|b| (m.key, b)))
        .collect();
    assert!(defaults["prompt_assistant.read_prompt"]);
    assert!(defaults["prompt_assistant.read_edits"]);
    assert!(defaults["prompt_assistant.read_project_instructions"]);
    assert!(defaults["prompt_assistant.read_versions"]);
    assert!(!defaults["prompt_assistant.read_memory"]);
    assert!(defaults["prompt_assistant.read_other_projects"]);
    assert!(defaults["prompt_assistant.capture"]);
}

#[test]
fn descriptions_are_short_and_name_no_internal_tool() {
    for meta in entries() {
        let words = meta.description.split_whitespace().count();
        assert!(
            words <= 30,
            "'{}' description runs to {words} words; say it in 30 or fewer",
            meta.key
        );
        assert!(
            !meta.description.contains('`'),
            "'{}' description names an internal identifier, got: {}",
            meta.key,
            meta.description
        );
    }
}

#[test]
fn scalar_defaults_match_spec_table() {
    let owned = entries();
    let by_key: std::collections::HashMap<&str, &SettingMeta> =
        owned.iter().map(|m| (m.key, m)).collect();

    assert_eq!(
        by_key["prompt_assistant.model"].default,
        serde_json::json!("claude-sonnet-5")
    );
    assert_eq!(
        by_key["prompt_assistant.codex_model"].default,
        serde_json::json!("gpt-6-astra")
    );
    assert_eq!(
        by_key["assistant.provider"].default,
        serde_json::json!("anthropic")
    );
    assert_eq!(
        by_key["prompt_assistant.effort"].default,
        serde_json::json!("medium")
    );
    assert_eq!(
        by_key["prompt_assistant.may_apply"].default,
        serde_json::json!("suggest")
    );
    assert_eq!(
        by_key["prompt_assistant.max_turns"].default,
        serde_json::json!(8)
    );
    assert_eq!(
        by_key["prompt_assistant.daily_budget_usd"].default,
        serde_json::json!(1.0)
    );
    assert_eq!(
        by_key["prompt_assistant.keep_days"].default,
        serde_json::json!(30)
    );
    assert_eq!(
        by_key["prompt_assistant.instructions"].default,
        serde_json::Value::String(DEFAULT_PROMPT_ASSISTANT_INSTRUCTIONS.to_owned())
    );
}

#[test]
fn bounds_match_spec_table() {
    let owned = entries();
    let by_key: std::collections::HashMap<&str, &SettingMeta> =
        owned.iter().map(|m| (m.key, m)).collect();

    let max_turns = by_key["prompt_assistant.max_turns"];
    assert_eq!(max_turns.min_i64, Some(1));
    assert_eq!(max_turns.max_i64, Some(40));

    let keep_days = by_key["prompt_assistant.keep_days"];
    assert_eq!(keep_days.min_i64, Some(1));
    assert_eq!(keep_days.max_i64, Some(365));

    let budget = by_key["prompt_assistant.daily_budget_usd"];
    assert_eq!(budget.min_f64, Some(0.0));
    assert_eq!(budget.max_f64, Some(100.0));

    let instructions = by_key["prompt_assistant.instructions"];
    assert_eq!(instructions.max_len, Some(16_384));
}

#[test]
fn effort_and_may_apply_enum_variants_match_spec_table() {
    let owned = entries();
    let by_key: std::collections::HashMap<&str, &SettingMeta> =
        owned.iter().map(|m| (m.key, m)).collect();

    assert_eq!(
        by_key["prompt_assistant.effort"].enum_variants,
        Some(["low", "medium", "high"].as_slice())
    );
    assert_eq!(
        by_key["prompt_assistant.may_apply"].enum_variants,
        Some(["suggest", "apply"].as_slice())
    );
    assert_eq!(
        by_key["assistant.provider"].enum_variants,
        Some(["anthropic", "codex"].as_slice())
    );
}

#[test]
fn every_default_value_passes_its_own_validator() {
    for meta in entries() {
        let result = crate::settings::validate_setting_value(meta.key, meta.default.clone());
        assert!(
            result.is_ok(),
            "default for '{}' must pass its own validator: {result:?}",
            meta.key
        );
    }
}

#[test]
fn instructions_default_is_non_blank_and_names_the_operator_contract() {
    assert!(!DEFAULT_PROMPT_ASSISTANT_INSTRUCTIONS.trim().is_empty());
    assert!(DEFAULT_PROMPT_ASSISTANT_INSTRUCTIONS.contains("Never"));
    assert!(DEFAULT_PROMPT_ASSISTANT_INSTRUCTIONS.contains("operator applies"));
}
