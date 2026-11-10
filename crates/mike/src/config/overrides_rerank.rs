use super::schema::ProxyConfig;

pub(super) fn apply_rerank_override(
    cfg: &mut ProxyConfig,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    crate::analysis::rerank::assign_rerank_key(&mut cfg.rerank, key, value)
}
