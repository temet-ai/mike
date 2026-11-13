use crate::store::persistence::PersistHandle;

const KEY: &str = "change_record";

pub async fn change_record_enabled(persist: &PersistHandle) -> bool {
    match persist.fetch_feature_toggles().await {
        Ok(rows) => rows
            .iter()
            .any(|(key, enabled, operator_set)| key == KEY && *enabled && *operator_set),
        Err(err) => {
            tracing::warn!(error = %err, "change record: could not read the switch, writing nothing");
            false
        }
    }
}
