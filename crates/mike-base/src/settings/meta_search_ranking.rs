use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};
use crate::search_ranking::{
    FLOOR_MAX, FLOOR_MIN, KEY_KIND_WEIGHTS, KEY_LEXICAL_WEIGHT, KEY_SEMANTIC_FLOOR,
    KEY_SEMANTIC_WEIGHT, KEY_SEMANTIC_WINDOW, RankingConfig, WEIGHT_MAX, WEIGHT_MIN, WINDOW_MAX,
    WINDOW_MIN,
};

const GROUP: &str = "Search ranking";

pub fn entries() -> Vec<SettingMeta> {
    let defaults = RankingConfig::default();
    vec![
        float(
            KEY_LEXICAL_WEIGHT,
            "Keyword match weight",
            "How much a keyword match counts when search results are ordered. The overnight tuner may change it (source rerank_tuner) when rerank.tuning.enabled is on.",
            defaults.lexical_weight,
            (WEIGHT_MIN, WEIGHT_MAX),
        ),
        float(
            KEY_SEMANTIC_WEIGHT,
            "Meaning match weight",
            "How much a match on meaning counts when search results are ordered. The overnight tuner may change it (source rerank_tuner) when rerank.tuning.enabled is on.",
            defaults.semantic_weight,
            (WEIGHT_MIN, WEIGHT_MAX),
        ),
        optional_float(
            KEY_SEMANTIC_FLOOR,
            "Meaning match floor",
            "Meaning matches scoring below this are ignored. Empty uses the embedding model's own floor; a number overrides it. The overnight tuner may change it (source rerank_tuner) when rerank.tuning.enabled is on.",
            (FLOOR_MIN, FLOOR_MAX),
        ),
        optional_float(
            KEY_SEMANTIC_WINDOW,
            "Meaning match window",
            "Meaning matches further than this below the best one are ignored. Empty uses the embedding model's own window; a number overrides it. The overnight tuner may change it (source rerank_tuner) when rerank.tuning.enabled is on.",
            (WINDOW_MIN, WINDOW_MAX),
        ),
        SettingMeta {
            key: KEY_KIND_WEIGHTS,
            group: GROUP,
            kind: SettingKind::Json,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::json!({}),
            label: "Weight per result kind",
            description: "A JSON object of result kind to a weight from 0 to 5; a kind left out counts 1, except seat (material written by fleet seats), which counts 0.5 and is never set above 0.9, so it stays below the operator's own material. The overnight tuner may change it (source rerank_tuner) when rerank.tuning.enabled is on.",
            min_i64: None,
            max_i64: None,
            min_f64: Some(WEIGHT_MIN),
            max_f64: Some(WEIGHT_MAX),
            max_len: None,
            enum_variants: None,
        },
    ]
}

fn optional_float(
    key: &'static str,
    label: &'static str,
    description: &'static str,
    range: (f64, f64),
) -> SettingMeta {
    SettingMeta {
        default: serde_json::Value::Null,
        ..float(key, label, description, 0.0, range)
    }
}

fn float(
    key: &'static str,
    label: &'static str,
    description: &'static str,
    default: f64,
    (min, max): (f64, f64),
) -> SettingMeta {
    SettingMeta {
        key,
        group: GROUP,
        kind: SettingKind::Float,
        scope: SettingScope::Global,
        applies: SettingApplies::NextMessage,
        default: serde_json::json!(default),
        label,
        description,
        min_i64: None,
        max_i64: None,
        min_f64: Some(min),
        max_f64: Some(max),
        max_len: None,
        enum_variants: None,
    }
}
