use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub fn entries() -> Vec<SettingMeta> {
    let mut v = server_entries();
    v.extend(limit_entries());
    v
}

fn server_entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            key: "rerank.enabled",
            group: "Reranker",
            kind: SettingKind::Bool,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::Bool(false),
            label: "Enable reranker",
            description: "Reorder search results with a reranker model. Each search sends the query and the candidate passages to rerank.url; a URL on another machine needs semantic.allow_remote_embedding. When the reranker does not answer in time, results keep their plain order.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "rerank.url",
            group: "Reranker",
            kind: SettingKind::String,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::String(
                crate::analysis::rerank::DEFAULT_RERANK_URL.to_owned(),
            ),
            label: "Reranker URL",
            description: "Base URL of the llama.cpp llama-server running with --reranking. Mike posts to /v1/rerank and reads /health.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(512),
            enum_variants: None,
        },
        SettingMeta {
            key: "rerank.model",
            group: "Reranker",
            kind: SettingKind::String,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::String(
                crate::analysis::rerank::DEFAULT_RERANK_MODEL.to_owned(),
            ),
            label: "Reranker model",
            description: "Model name sent with each rerank request.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(256),
            enum_variants: None,
        },
    ]
}

fn limit_entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            key: "rerank.timeout_ms",
            group: "Reranker",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::from(800u64),
            label: "Reranker timeout (ms)",
            description: "Hard limit for one rerank request. A search that is not reranked within this time returns its plain order.",
            min_i64: Some(50),
            max_i64: Some(30_000),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "rerank.candidates",
            group: "Reranker",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::NextMessage,
            default: serde_json::Value::from(50u64),
            label: "Reranker candidates",
            description: "How many top search results are sent to the reranker. Only these are reordered; the reranker finds nothing the first search missed.",
            min_i64: Some(1),
            max_i64: Some(200),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "rerank.tuning.enabled",
            group: "Overnight reranking",
            kind: SettingKind::Bool,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::Bool(false),
            label: "Overnight reranker tuning",
            description: "Let the overnight worker grade the day's searches with the reranker and adjust ranking settings when the graded results improve. Each change is recorded as a setting with the source rerank_tuner.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
    ]
}
