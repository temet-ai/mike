use crate::config::ProxyConfig;
use crate::config::overrides::apply_settings_overrides;
use crate::store::persistence::{PersistError, PersistHandle};

pub async fn fold_db_settings(persist: Option<&PersistHandle>, config: &mut ProxyConfig) {
    let Some(handle) = persist else {
        return;
    };
    let rows = handle.fetch_settings().await;
    apply_fetched_settings(config, rows);
}

fn apply_fetched_settings(
    config: &mut ProxyConfig,
    rows: Result<Vec<(String, serde_json::Value)>, PersistError>,
) {
    let rows = match rows {
        Ok(rows) => rows,
        Err(err) => {
            log_settings_fetch_failed(&err);
            return;
        }
    };
    apply_settings_overrides(config, &rows);
    log_settings_folded(rows.len());
}

fn log_settings_fetch_failed(err: &PersistError) {
    tracing::warn!(
        error = %err,
        "settings: failed to fetch DB overrides; ProxyConfig stays at compiled/TOML defaults"
    );
}

fn log_settings_folded(count: usize) {
    tracing::info!(
        count,
        "settings: DB overrides folded onto ProxyConfig (OnRestart keys)"
    );
}
