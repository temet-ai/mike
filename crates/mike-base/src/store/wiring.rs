use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::broadcast;

use super::RequestStore;
use super::models::ProxyEvent;
use super::retention::RetentionPolicy;
use crate::analysis::{AnalyzerRegistry, EmbeddingService};
use crate::store::persistence::PersistHandle;

impl RequestStore {
    pub fn new(retention: RetentionPolicy) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            retention,
            session_index: HashMap::new(),
            composable_session_index: HashMap::new(),
            established_persona_index: HashMap::new(),
            session_pending_reminders: HashMap::new(),
            session_latest_findings: HashMap::new(),
            session_meta: HashMap::new(),
            session_dedup_index: HashMap::new(),
            session_dedup_stats: HashMap::new(),
            analyzer_registry: None,
            analysis_event_tx: None,
            persist: None,
            embedding: None,
        }
    }

    pub fn set_persist(&mut self, persist: PersistHandle) {
        self.persist = Some(persist);
    }

    pub fn persist(&self) -> Option<&PersistHandle> {
        self.persist.as_ref()
    }

    pub fn set_embedding(
        &mut self,
        service: EmbeddingService,
        permits: Arc<tokio::sync::Semaphore>,
    ) {
        self.embedding = Some((service, permits));
    }

    pub fn embedding(&self) -> Option<(&EmbeddingService, &Arc<tokio::sync::Semaphore>)> {
        self.embedding.as_ref().map(|(s, p)| (s, p))
    }

    pub fn set_session_pinned(&mut self, session_id: &str, pinned: bool) {
        let meta = self.session_meta.entry(session_id.to_string()).or_default();
        meta.pinned = pinned;
    }

    pub fn set_session_ttl_override(&mut self, session_id: &str, ttl_override: Option<String>) {
        let meta = self.session_meta.entry(session_id.to_string()).or_default();
        meta.ttl_override = ttl_override;
    }

    pub fn set_session_title(&mut self, session_id: &str, title: String) {
        let meta = self.session_meta.entry(session_id.to_string()).or_default();
        meta.title = Some(title);
    }

    #[allow(dead_code)]
    pub fn session_id(&self, session_uuid: &str) -> Option<i64> {
        self.session_meta.get(session_uuid)?.session_id()
    }

    pub fn set_analysis(
        &mut self,
        registry: Arc<ArcSwap<AnalyzerRegistry>>,
        event_tx: broadcast::Sender<ProxyEvent>,
    ) {
        self.analyzer_registry = Some(registry);
        self.analysis_event_tx = Some(event_tx);
    }

    pub fn analysis_wired(
        &self,
    ) -> Option<(
        &Arc<ArcSwap<AnalyzerRegistry>>,
        &broadcast::Sender<ProxyEvent>,
    )> {
        match (&self.analyzer_registry, &self.analysis_event_tx) {
            (Some(r), Some(tx)) => Some((r, tx)),
            _ => None,
        }
    }
}
