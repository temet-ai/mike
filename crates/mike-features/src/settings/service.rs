use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::Mutex;
use uuid::Uuid;

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

#[derive(Debug, Clone, Default)]
pub struct MachineView {
    pub machine_uuid: Option<Uuid>,
    pub machine_name: Option<String>,
    pub own: HashMap<String, serde_json::Value>,
    pub shared: HashMap<String, serde_json::Value>,
}

impl MachineView {
    pub fn value(&self, key: &str) -> Option<&serde_json::Value> {
        self.own.get(key).or_else(|| self.shared.get(key))
    }

    pub fn source(&self, key: &str) -> super::ValueSource {
        super::value_source(key, &self.own, &self.shared)
    }
}

pub use super::validate::is_secret_setting_key;

pub struct SettingsService {
    pub overlay: Arc<ArcSwap<SettingsOverlay>>,

    write_lock: Arc<Mutex<SettingsWriteGuard>>,

    pub persist: PersistHandle,

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

    pub async fn reconcile_on_startup_with_bundles(
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

    pub async fn set_with_source(
        &self,
        key: &str,
        value: serde_json::Value,
        source: &ChangeSource,
    ) -> Result<(serde_json::Value, Vec<i64>), SettingsServiceError> {
        self.set_for_machine_with_source(key, value, None, source)
            .await
    }

    pub async fn own_machine(&self) -> Option<crate::machine_identity::MachineIdentity> {
        crate::machine_identity::load_default()
            .await
            .inspect_err(|error| tracing::debug!(%error, "settings: no machine identity"))
            .ok()
    }

    pub async fn machine_view(
        &self,
        machine: Option<Uuid>,
    ) -> Result<MachineView, SettingsServiceError> {
        let own = self.own_machine().await;
        let machine_uuid = machine.or(own.as_ref().map(|m| m.machine_uuid));
        let machine_name = match (machine_uuid, &own) {
            (Some(uuid), Some(own)) if own.machine_uuid == uuid => Some(own.machine_name.clone()),
            (Some(uuid), _) => self.persist.machine_name(uuid).await?,
            (None, _) => None,
        };
        let own_values = match machine_uuid {
            Some(uuid) => self.persist.fetch_machine_settings(uuid).await?,
            None => Vec::new(),
        };
        let shared = self.persist.fetch_shared_settings().await?;
        Ok(MachineView {
            machine_uuid,
            machine_name,
            own: own_values.into_iter().collect(),
            shared: shared.into_iter().collect(),
        })
    }

    pub async fn clear_machine_value(
        &self,
        key: &str,
        machine: Option<Uuid>,
        source: &ChangeSource,
    ) -> Result<(Uuid, bool), SettingsServiceError> {
        if !super::is_machine_scoped(key) {
            return Err(SettingsServiceError::InvalidValue {
                key: key.to_owned(),
                reason: "only a machine-scoped setting has a machine's own value to clear"
                    .to_owned(),
            });
        }
        let target = self.target_machine(key, machine).await?;
        let source_json = source
            .clone()
            .with_detail("machine_uuid", serde_json::Value::from(target.to_string()))
            .to_setting_json(&current_build());
        let _guard = self.write_lock.lock().await;
        let deleted = self
            .persist
            .delete_machine_setting_with_source(key, target, &source_json)
            .await?;
        Ok((target, deleted))
    }

    async fn target_machine(
        &self,
        key: &str,
        machine: Option<Uuid>,
    ) -> Result<Uuid, SettingsServiceError> {
        match machine {
            Some(uuid) => Ok(uuid),
            None => self
                .own_machine()
                .await
                .map(|m| m.machine_uuid)
                .ok_or_else(|| SettingsServiceError::InvalidValue {
                    key: key.to_owned(),
                    reason: "this machine has no identity file (machine.json in the mike state \
                             directory), so a machine-scoped value has no machine to belong to; \
                             name the machine in the request"
                        .to_owned(),
                }),
        }
    }

    async fn write_target(
        &self,
        meta: &super::SettingMeta,
        machine: Option<Uuid>,
        source: &ChangeSource,
    ) -> Result<(Option<Uuid>, String), SettingsServiceError> {
        if machine.is_some() && meta.scope != super::SettingScope::Machine {
            return Err(SettingsServiceError::InvalidValue {
                key: meta.key.to_owned(),
                reason: "a machine was named, but this setting is shared by every machine"
                    .to_owned(),
            });
        }
        let target = match meta.scope {
            super::SettingScope::Machine => Some(self.target_machine(meta.key, machine).await?),
            _ => None,
        };
        let applies = serde_json::to_value(meta.applies).unwrap_or(serde_json::Value::Null);
        let mut source = source.clone().with_detail("applies", applies);
        if let Some(uuid) = target {
            source = source.with_detail("machine_uuid", serde_json::Value::from(uuid.to_string()));
        }
        Ok((target, source.to_setting_json(&current_build())))
    }

    async fn refresh_overlay(&self, key: &str, ids: &[i64]) {
        match self.persist.fetch_settings().await {
            Ok(rows) => {
                let new_overlay = apply_db_rows_to_overlay(rows);
                let new_overlay = self
                    .with_bundles(new_overlay, BundleCause::setting(key, ids.to_vec()))
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

    pub async fn set_for_machine_with_source(
        &self,
        key: &str,
        value: serde_json::Value,
        machine: Option<Uuid>,
        source: &ChangeSource,
    ) -> Result<(serde_json::Value, Vec<i64>), SettingsServiceError> {
        let validated = validated_value(key, value)?;
        let meta = super::SETTINGS_META
            .iter()
            .find(|m| m.key == key)
            .ok_or_else(|| SettingsServiceError::NotFound(key.to_owned()))?;
        let (target, source_json) = self.write_target(meta, machine, source).await?;

        let _guard = self.write_lock.lock().await;

        let ids = match target {
            Some(uuid) => {
                self.persist
                    .upsert_machine_setting_with_source(key, uuid, &validated, &source_json)
                    .await?
            }
            None => {
                self.persist
                    .upsert_setting_with_source(key, &validated, &source_json)
                    .await?
            }
        };

        if refreshes_overlay(meta) {
            self.refresh_overlay(key, &ids).await;
        }

        Ok((validated, ids))
    }

    pub async fn fetch_all(&self) -> Result<Vec<(String, serde_json::Value)>, PersistError> {
        self.persist.fetch_settings().await
    }
}

fn refreshes_overlay(meta: &super::SettingMeta) -> bool {
    meta.scope == super::SettingScope::Session
        || meta.applies == super::SettingApplies::NextCycle
        || meta.applies == super::SettingApplies::NextMessage
}

fn validated_value(
    key: &str,
    value: serde_json::Value,
) -> Result<serde_json::Value, SettingsServiceError> {
    if is_secret_setting_key(key) {
        return Err(SettingsServiceError::InvalidValue {
            key: key.to_owned(),
            reason: "secrets are never stored in the settings table; set the value in the \
                     process environment and store only the variable name in a `_env` setting"
                .to_owned(),
        });
    }
    validate_setting_value(key, value).map_err(|reason| {
        if reason.starts_with("unknown setting key:") {
            SettingsServiceError::NotFound(key.to_owned())
        } else {
            SettingsServiceError::InvalidValue {
                key: key.to_owned(),
                reason,
            }
        }
    })
}

#[cfg(test)]
pub mod tests {
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
