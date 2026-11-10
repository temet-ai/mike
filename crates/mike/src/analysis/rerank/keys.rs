use super::RerankConfig;

pub(crate) fn assign_rerank_key(
    config: &mut RerankConfig,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    if let Some(applied) = config.nightly.assign(key, value) {
        return applied;
    }
    match key {
        "rerank.enabled" => set(&mut config.enabled, value.as_bool()),
        "rerank.url" => set(
            &mut config.url,
            value.as_str().map(|v| v.trim_end_matches('/').to_owned()),
        ),
        "rerank.model" => set(&mut config.model, value.as_str().map(str::to_owned)),
        "rerank.timeout_ms" => set(&mut config.timeout_ms, value.as_u64().map(|v| v.max(1))),
        "rerank.cold_timeout_ms" => set(
            &mut config.cold_timeout_ms,
            value.as_u64().map(|v| v.max(1)),
        ),
        "rerank.min_query_words" => set(
            &mut config.min_query_words,
            value
                .as_u64()
                .and_then(|v| usize::try_from(v).ok())
                .map(|v| v.max(1)),
        ),
        "rerank.candidates" => set(
            &mut config.candidates,
            value
                .as_u64()
                .and_then(|v| usize::try_from(v).ok())
                .map(|v| v.max(1)),
        ),
        "rerank.tuning.enabled" => set(&mut config.tuning_enabled, value.as_bool()),
        "rerank.rescue_score" => set(&mut config.rescue_score, value.as_f64()),
        "rerank.search_no_project" => set(&mut config.search_no_project, value.as_bool()),
        "rerank.rescue_min_kept" => set(
            &mut config.rescue_min_kept,
            value.as_u64().and_then(|v| usize::try_from(v).ok()),
        ),
        _ => false,
    }
}

fn set<T>(slot: &mut T, value: Option<T>) -> bool {
    match value {
        Some(v) => {
            *slot = v;
            true
        }
        None => false,
    }
}
