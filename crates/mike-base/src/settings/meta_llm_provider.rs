use super::local_llm::{
    DEFAULT_LOCAL_KEY_ENV, DEFAULT_SIDE_CALL_MODELS, LOCAL_KEY_ENV_KEY, LOCAL_URL_KEY,
    SIDE_CALL_MODELS_KEY,
};
use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};
use crate::llm::provider::{LlmProvider, PROVIDER_KEY, SLOTS};

const GROUP: &str = "LLM provider";

const LABELS: [&str; 9] = [
    "OpenRouter, fast tier",
    "OpenRouter, standard tier",
    "OpenRouter, strong tier",
    "Claude, fast tier",
    "Claude, standard tier",
    "Claude, strong tier",
    "Codex, fast tier",
    "Codex, standard tier",
    "Codex, strong tier",
];

const DESCRIPTIONS: [&str; 9] = [
    "Model the fast roles use while the provider is openrouter: session judge, lesson proposer and lesson-relations adjudicator. An OpenRouter model id.",
    "Model the standard roles use while the provider is openrouter: turn judge. An OpenRouter model id.",
    "Model the strong roles use while the provider is openrouter: steerers, steerer sub-judges, turn guard labeller and lesson-relations proposer. An OpenRouter model id.",
    "Model the fast roles use while the provider is claude: session judge, lesson proposer and lesson-relations adjudicator. Written claude/<model>, for example claude/haiku or a full model name.",
    "Model the standard roles use while the provider is claude: turn judge. Written claude/<model>.",
    "Model the strong roles use while the provider is claude: steerers, steerer sub-judges, turn guard labeller and lesson-relations proposer. Written claude/<model>.",
    "Model the fast roles use while the provider is codex: session judge, lesson proposer and lesson-relations adjudicator. Written codex/<model>.",
    "Model the standard roles use while the provider is codex: turn judge. Written codex/<model>.",
    "Model the strong roles use while the provider is codex: steerers, steerer sub-judges, turn guard labeller and lesson-relations proposer. Written codex/<model>.",
];

fn model_entry(index: usize) -> Option<SettingMeta> {
    let slot = SLOTS.get(index)?;
    Some(SettingMeta {
        key: slot.key,
        group: GROUP,
        kind: SettingKind::String,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default: serde_json::Value::String(slot.default.to_owned()),
        label: LABELS.get(index).copied()?,
        description: DESCRIPTIONS.get(index).copied()?,
        min_i64: None,
        max_i64: None,
        min_f64: None,
        max_f64: None,
        max_len: Some(256),
        enum_variants: None,
    })
}

pub fn entries() -> Vec<SettingMeta> {
    let mut entries = vec![SettingMeta {
        key: PROVIDER_KEY,
        group: GROUP,
        kind: SettingKind::Enum,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default: serde_json::Value::String(LlmProvider::PerRole.as_str().to_owned()),
        label: "Provider for every backend LLM role",
        description: "per_role keeps each role's own model setting. openrouter, claude or codex sends every role (steerers, turn judge, session judge, lesson proposer, lesson relations) to that provider's models below, whatever the role's own model says. claude and codex use the login already on this machine, so no OpenRouter key is needed. A provider whose login is missing shows as inert on the home page; mike does not fall back to another provider. Evaluation runs keep the model they were asked to test.",
        min_i64: None,
        max_i64: None,
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: Some(LlmProvider::VARIANTS),
    }];
    entries.extend((0..SLOTS.len()).filter_map(model_entry));
    entries.extend(local_entries());
    entries
}

fn local_entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            key: LOCAL_URL_KEY,
            group: GROUP,
            kind: SettingKind::String,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::String(String::new()),
            label: "Local server URL",
            description: "Base URL of an OpenAI-compatible local server such as llama-server or Ollama, for example http://mac.local:8080/v1. Models written local/<name> are sent to {url}/chat/completions. Empty means no local server is configured.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(512),
            enum_variants: None,
        },
        SettingMeta {
            key: LOCAL_KEY_ENV_KEY,
            group: GROUP,
            kind: SettingKind::String,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::String(DEFAULT_LOCAL_KEY_ENV.to_owned()),
            label: "Local server API key env var",
            description: "Name of the environment variable the proxy reads the local server's bearer API key from at call time. A missing or empty variable sends no key, which suits a llama-server started without --api-key.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(256),
            enum_variants: None,
        },
        SettingMeta {
            key: SIDE_CALL_MODELS_KEY,
            group: "Side calls",
            kind: SettingKind::StringList,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::Array(
                DEFAULT_SIDE_CALL_MODELS
                    .iter()
                    .map(|m| serde_json::Value::String((*m).to_owned()))
                    .collect(),
            ),
            label: "Side call models, in order",
            description: "Models a side call tries in turn; a model that is busy, failing or over the hourly cap is skipped for the next.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(256),
            enum_variants: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_provider_model_slot_has_a_setting_with_its_default() {
        let entries = entries();
        for slot in SLOTS {
            let entry = entries.iter().find(|e| e.key == slot.key);
            assert_eq!(
                entry.map(|e| e.default.clone()),
                Some(serde_json::Value::String(slot.default.to_owned())),
                "{}",
                slot.key
            );
        }
    }

    #[test]
    fn provider_setting_defaults_to_per_role_and_lists_every_provider() {
        let entries = entries();
        let provider = entries.iter().find(|e| e.key == PROVIDER_KEY);
        assert_eq!(
            provider.map(|e| e.default.clone()),
            Some(serde_json::json!("per_role"))
        );
        assert_eq!(
            provider.and_then(|e| e.enum_variants),
            Some(LlmProvider::VARIANTS)
        );
    }

    #[test]
    fn local_endpoint_settings_default_to_not_configured_with_the_standard_key_env() {
        let entries = entries();
        let find = |key: &str| {
            entries
                .iter()
                .find(|e| e.key == key)
                .map(|e| e.default.clone())
        };
        assert_eq!(find(LOCAL_URL_KEY), Some(serde_json::json!("")));
        assert_eq!(
            find(LOCAL_KEY_ENV_KEY),
            Some(serde_json::json!("MIKE_LOCAL_LLM_API_KEY"))
        );
    }

    #[test]
    fn labels_and_descriptions_cover_every_slot() {
        assert_eq!(LABELS.len(), SLOTS.len());
        assert_eq!(DESCRIPTIONS.len(), SLOTS.len());
    }
}
