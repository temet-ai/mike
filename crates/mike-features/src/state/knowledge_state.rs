use std::sync::Arc;

use crate::change_log::ChangeSource;
use crate::eval_metrics::drift_alarms::EvalDriftAlarms;
use crate::semantic::knowledge_state::{KnowledgeFuture, KnowledgeState, SettingWriter};
use crate::settings::service::SettingsService;

use super::AppState;

impl SettingWriter for SettingsService {
    fn write<'a>(
        &'a self,
        key: &'a str,
        value: serde_json::Value,
        source: &'a ChangeSource,
    ) -> KnowledgeFuture<'a, Result<serde_json::Value, String>> {
        Box::pin(async move {
            self.set_with_source(key, value, source)
                .await
                .map(|(validated, _)| validated)
                .map_err(|error| error.to_string())
        })
    }
}

impl AppState {
    pub fn knowledge_state(&self) -> KnowledgeState {
        KnowledgeState {
            base: self.base.clone(),
            semantic: self.semantic.clone(),
            embedding_service: self.embedding_service.clone(),
            rerank_service: self.rerank_service.clone(),
            settings: self
                .settings_service
                .clone()
                .map(|service| service as Arc<dyn SettingWriter>),
            drift_alarms: Arc::new(EvalDriftAlarms),
        }
    }
}
