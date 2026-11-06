use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::Mutex;

use super::{FeatureToggles, set_toggle_enabled};
use crate::change_log::{ChangeSource, current_build};
use crate::config::FeatureToggleWriteGuard;
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, thiserror::Error)]
pub enum FeatureToggleServiceError {
    #[error("unknown feature toggle key: {0}")]
    NotFound(String),

    #[error("persistence error: {0}")]
    Persistence(#[from] PersistError),
}

pub struct FeatureToggleService {
    overlay: Arc<ArcSwap<FeatureToggles>>,

    write_lock: Arc<Mutex<FeatureToggleWriteGuard>>,

    persist: PersistHandle,
}

impl FeatureToggleService {
    pub async fn reconcile_on_startup(
        overlay: Arc<ArcSwap<FeatureToggles>>,
        write_lock: Arc<Mutex<FeatureToggleWriteGuard>>,
        persist: PersistHandle,
    ) -> anyhow::Result<Arc<Self>> {
        let service = Arc::new(Self {
            overlay,
            write_lock,
            persist,
        });

        {
            let _guard = service.write_lock.lock().await;
            let db_rows = service.persist.fetch_feature_toggles().await?;
            service.apply_db_rows(db_rows);
        }

        Ok(service)
    }

    pub async fn set(
        &self,
        key: &str,
        enabled: bool,
    ) -> Result<(&'static str, bool), FeatureToggleServiceError> {
        let (canonical, enabled, _ids) = self
            .set_with_source(key, enabled, &ChangeSource::internal())
            .await?;
        Ok((canonical, enabled))
    }

    pub(crate) async fn set_with_source(
        &self,
        key: &str,
        enabled: bool,
        source: &ChangeSource,
    ) -> Result<(&'static str, bool, Vec<i64>), FeatureToggleServiceError> {
        let meta = super::TOGGLE_META
            .iter()
            .find(|m| m.key == key)
            .ok_or_else(|| FeatureToggleServiceError::NotFound(key.to_owned()))?;

        let applies = serde_json::to_value(meta.applies).unwrap_or(serde_json::Value::Null);
        let source_json = source
            .clone()
            .with_detail("applies", applies)
            .to_setting_json(&current_build());

        let _guard = self.write_lock.lock().await;

        let ids = self
            .persist
            .upsert_feature_toggle_with_source(key, enabled, &source_json)
            .await?;

        let current = self.overlay.load_full();
        if let Some(updated) = set_toggle_enabled(&current, key, enabled) {
            self.overlay.store(Arc::new(updated));
        }

        Ok((meta.key, enabled, ids))
    }

    fn apply_db_rows(&self, rows: Vec<(String, bool, bool)>) {
        let mut toggles = FeatureToggles::default();
        for (key, enabled, operator_set) in rows {
            if !operator_set {
                continue;
            }
            if let Some(updated) = set_toggle_enabled(&toggles, &key, enabled) {
                toggles = updated;
            }
        }
        self.overlay.store(Arc::new(toggles));
    }
}
