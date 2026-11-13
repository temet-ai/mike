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
    entries
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
    fn labels_and_descriptions_cover_every_slot() {
        assert_eq!(LABELS.len(), SLOTS.len());
        assert_eq!(DESCRIPTIONS.len(), SLOTS.len());
    }
}
