use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::Mutex;

use super::{SettingsOverlay, apply_db_rows_to_overlay, validate_setting_value};
use crate::change_log::{ChangeSource, current_build};
use crate::config::SettingsWriteGuard;
use crate::eval::bundles::{BundleCause, BundlePublisher};
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, thiserror::Error)]
pub enum SettingsServiceError {
    #[error("unknown setting key: {0}")]
    NotFound(String),

    #[error("invalid value for setting '{key}': {reason}")]
    InvalidValue { key: String, reason: String },

    #[error("persistence error: {0}")]
    Persistence(#[from] PersistError),
}

const SECRET_KEY_SEGMENTS: &[&str] = &["key", "secret", "token", "password", "credential"];

pub(crate) fn is_secret_setting_key(key: &str) -> bool {
    if key.ends_with("_env") {
        return false;
    }
    key.split(['.', '_'])
        .any(|segment| SECRET_KEY_SEGMENTS.contains(&segment))
}

pub struct SettingsService {
    pub(super) overlay: Arc<ArcSwap<SettingsOverlay>>,

    write_lock: Arc<Mutex<SettingsWriteGuard>>,

    pub(super) persist: PersistHandle,

    publisher: Option<Arc<BundlePublisher>>,
}

impl SettingsService {
    pub async fn reconcile_on_startup(
        overlay: Arc<ArcSwap<SettingsOverlay>>,
        write_lock: Arc<Mutex<SettingsWriteGuard>>,
        persist: PersistHandle,
    ) -> anyhow::Result<Arc<Self>> {
        Self::reconcile_on_startup_with_bundles(overlay, write_lock, persist, None).await
    }

    pub(crate) async fn reconcile_on_startup_with_bundles(
        overlay: Arc<ArcSwap<SettingsOverlay>>,
        write_lock: Arc<Mutex<SettingsWriteGuard>>,
        persist: PersistHandle,
        publisher: Option<Arc<BundlePublisher>>,
    ) -> anyhow::Result<Arc<Self>> {
        let service = Arc::new(Self {
            overlay,
            write_lock,
            persist,
            publisher,
        });

        {
            let _guard = service.write_lock.lock().await;
            let db_rows = service.persist.fetch_settings().await?;
            let new_overlay = apply_db_rows_to_overlay(db_rows);
            let new_overlay = service
                .with_bundles(new_overlay, BundleCause::startup())
                .await;
            service.overlay.store(Arc::new(new_overlay));
        }

        Ok(service)
    }

    async fn with_bundles(
        &self,
        mut overlay: SettingsOverlay,
        cause: BundleCause,
    ) -> SettingsOverlay {
        if let Some(publisher) = &self.publisher {
            overlay.bundles = Arc::new(publisher.resolve_ids(&overlay, cause).await);
        }
        overlay
    }

    pub async fn set(
        &self,
        key: &str,
        value: serde_json::Value,
    ) -> Result<serde_json::Value, SettingsServiceError> {
        let (validated, _ids) = self
            .set_with_source(key, value, &ChangeSource::internal())
            .await?;
        Ok(validated)
    }

    pub(crate) async fn set_with_source(
        &self,
        key: &str,
        value: serde_json::Value,
        source: &ChangeSource,
    ) -> Result<(serde_json::Value, Vec<i64>), SettingsServiceError> {
        if is_secret_setting_key(key) {
            return Err(SettingsServiceError::InvalidValue {
                key: key.to_owned(),
                reason: "secrets are never stored in the settings table; set the value in the \
                         process environment and store only the variable name in a `_env` setting"
                    .to_owned(),
            });
        }

        let validated = validate_setting_value(key, value).map_err(|reason| {
            if reason.starts_with("unknown setting key:") {
                SettingsServiceError::NotFound(key.to_owned())
            } else {
                SettingsServiceError::InvalidValue {
                    key: key.to_owned(),
                    reason,
                }
            }
        })?;

        let meta = super::SETTINGS_META
            .iter()
            .find(|m| m.key == key)
            .ok_or_else(|| SettingsServiceError::NotFound(key.to_owned()))?;
        let applies = serde_json::to_value(meta.applies).unwrap_or(serde_json::Value::Null);
        let source_json = source
            .clone()
            .with_detail("applies", applies)
            .to_setting_json(&current_build());

        let _guard = self.write_lock.lock().await;

        let ids = self
            .persist
            .upsert_setting_with_source(key, &validated, &source_json)
            .await?;

        if meta.scope == super::SettingScope::Session
            || meta.applies == super::SettingApplies::NextCycle
            || meta.applies == super::SettingApplies::NextMessage
        {
            match self.persist.fetch_settings().await {
                Ok(rows) => {
                    let new_overlay = apply_db_rows_to_overlay(rows);
                    let new_overlay = self
                        .with_bundles(new_overlay, BundleCause::setting(key, ids.clone()))
                        .await;
                    self.overlay.store(Arc::new(new_overlay));
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        key,
                        "settings: DB write succeeded but re-fetch for overlay update failed; overlay may lag by one tick"
                    );
                }
            }
        }

        Ok((validated, ids))
    }

    pub(crate) async fn fetch_all(&self) -> Result<Vec<(String, serde_json::Value)>, PersistError> {
        self.persist.fetch_settings().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_secret_setting_key_flags_credential_shaped_keys() {
        assert!(is_secret_setting_key("steerer.openrouter_api_key"));
        assert!(is_secret_setting_key("integrations.slack.token"));
        assert!(is_secret_setting_key("db.password"));
        assert!(is_secret_setting_key("webhook_secret"));
    }

    #[test]
    fn is_secret_setting_key_allows_env_references_and_token_counts() {
        assert!(!is_secret_setting_key("steerer.openrouter_api_key_env"));
        assert!(!is_secret_setting_key("llm.max_output_tokens"));
        assert!(!is_secret_setting_key("reminders.max_lessons_to_inject"));
    }
}
