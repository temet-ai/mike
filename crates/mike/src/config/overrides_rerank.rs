use super::schema::ProxyConfig;

pub(super) fn apply_rerank_override(
    cfg: &mut ProxyConfig,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    let rerank = &mut cfg.rerank;
    match key {
        "rerank.enabled" => assign(&mut rerank.enabled, value.as_bool()),
        "rerank.url" => assign(&mut rerank.url, value.as_str().map(str::to_owned)),
        "rerank.model" => assign(&mut rerank.model, value.as_str().map(str::to_owned)),
        "rerank.timeout_ms" => assign(&mut rerank.timeout_ms, value.as_u64()),
        "rerank.candidates" => assign(
            &mut rerank.candidates,
            value.as_u64().and_then(|v| usize::try_from(v).ok()),
        ),
        "rerank.tuning.enabled" => assign(&mut rerank.tuning_enabled, value.as_bool()),
        _ => return false,
    }
    true
}

fn assign<T>(slot: &mut T, value: Option<T>) {
    if let Some(v) = value {
        *slot = v;
    }
}
