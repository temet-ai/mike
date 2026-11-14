use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::analysis::{EmbeddingService, RerankService};
use crate::base_state::BaseState;
use crate::change_log::ChangeSource;
use crate::store::persistence::{PersistError, PersistHandle};

use super::SemanticHandle;
use super::drift_job::DriftLine;

pub type KnowledgeFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait SettingWriter: Send + Sync {
    fn write<'a>(
        &'a self,
        key: &'a str,
        value: serde_json::Value,
        source: &'a ChangeSource,
    ) -> KnowledgeFuture<'a, Result<serde_json::Value, String>>;
}

pub trait DriftAlarms: Send + Sync {
    fn raise<'a>(
        &'a self,
        persist: &'a PersistHandle,
        embedder_key: &'a str,
        lines: &'a [DriftLine],
    ) -> KnowledgeFuture<'a, Result<(), PersistError>>;
}

pub struct NoDriftAlarms;

impl DriftAlarms for NoDriftAlarms {
    fn raise<'a>(
        &'a self,
        _persist: &'a PersistHandle,
        _embedder_key: &'a str,
        _lines: &'a [DriftLine],
    ) -> KnowledgeFuture<'a, Result<(), PersistError>> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct KnowledgeState {
    pub base: BaseState,
    pub semantic: SemanticHandle,
    pub embedding_service: EmbeddingService,
    pub rerank_service: RerankService,
    pub settings: Option<Arc<dyn SettingWriter>>,
    pub drift_alarms: Arc<dyn DriftAlarms>,
    pub lesson_pairs: Option<Arc<dyn crate::rerank_nightly::PairsPhase>>,
}

impl std::ops::Deref for KnowledgeState {
    type Target = BaseState;

    fn deref(&self) -> &BaseState {
        &self.base
    }
}

impl KnowledgeState {
    pub fn semantic(&self) -> SemanticHandle {
        self.semantic.clone()
    }

    pub fn embedding_service(&self) -> EmbeddingService {
        self.embedding_service.clone()
    }

    pub fn rerank_service(&self) -> RerankService {
        self.rerank_service.clone()
    }

    pub fn settings(&self) -> Option<&Arc<dyn SettingWriter>> {
        self.settings.as_ref()
    }
}
