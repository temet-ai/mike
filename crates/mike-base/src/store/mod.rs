pub mod dedup_ops;
pub mod composable_session;
pub mod entry_ops;
pub mod eviction;
pub mod memory_stats;
pub mod models;
pub mod operator_turns;
pub mod persistence;
pub mod reminders;
pub mod request_completion;
pub mod request_stub;
pub mod request_summary;
pub mod retention;
pub mod session_index;
pub mod terminal_snapshot;
pub mod tool_result_attach;
pub mod updater;
pub mod views;
pub mod wiring;

#[cfg(any(test, feature = "test-seams"))]
pub mod tests;

#[cfg(any(test, feature = "test-seams"))]
pub use persistence::test_support;

pub use views::{
    AttachToolResultsOutcome, DiagnosticStatsView, FindingView, MemoryBreakdownView, MemoryEntryRow,
};

use arc_swap::ArcSwap;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::analysis::{AnalyzerRegistry, EmbeddingService};
use models::{InterceptedRequest, ProxyEvent, RequestId};
use persistence::PersistHandle;
use retention::RetentionPolicy;

pub struct RequestStore {
    entries: HashMap<RequestId, InterceptedRequest>,
    order: VecDeque<RequestId>,
    retention: RetentionPolicy,
    session_index: HashMap<String, VecDeque<RequestId>>,
    composable_session_index: HashMap<String, ()>,
    established_persona_index: HashMap<String, Option<String>>,
    session_pending_reminders: HashMap<String, VecDeque<models::SystemReminder>>,
    session_latest_findings: HashMap<String, (RequestId, Vec<models::Diagnostic>)>,
    session_meta: HashMap<String, models::SessionMeta>,
    pub session_dedup_index: HashMap<String, crate::wire::read_dedup::SessionDedupIndex>,
    pub session_dedup_stats: HashMap<String, crate::wire::read_dedup::SessionDedupStats>,
    analyzer_registry: Option<Arc<ArcSwap<AnalyzerRegistry>>>,
    analysis_event_tx: Option<broadcast::Sender<ProxyEvent>>,
    persist: Option<PersistHandle>,
    embedding: Option<(EmbeddingService, Arc<tokio::sync::Semaphore>)>,
}

const MAX_PENDING_REMINDERS_PER_SESSION: usize = 16;

pub struct RequestStoreShedHandle {
    store: std::sync::Arc<tokio::sync::RwLock<RequestStore>>,
}

impl std::fmt::Debug for RequestStoreShedHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestStoreShedHandle").finish()
    }
}

impl RequestStoreShedHandle {
    pub fn new(store: std::sync::Arc<tokio::sync::RwLock<RequestStore>>) -> Self {
        Self { store }
    }
}

#[async_trait::async_trait]
impl crate::memory_cap::Sheddable for RequestStoreShedHandle {
    fn priority(&self) -> crate::memory_cap::ShedPriority {
        crate::memory_cap::ShedPriority::Medium
    }

    async fn shed(&self, target_bytes: u64) -> Option<crate::memory_cap::sheddable::ShedReport> {
        let mut guard = self.store.write().await;
        let (bytes_freed, entries_evicted) = guard.shed_to_target(target_bytes);
        let more_available = guard.terminal_entry_count() > 0;
        drop(guard);

        if bytes_freed > 0 {
            crate::observability::shed::emit(
                crate::observability::shed::MemoryShedEvent {
                    event: "memory.shed",
                    path: crate::observability::shed::ShedPath::StoreInsert,
                    reason: crate::observability::shed::ShedReason::EvictExisting,
                    bytes: bytes_freed,
                    limit_bytes: target_bytes,
                    request_id: None,
                    session_id: None,
                    note: "store_lru_evicted_to_fit",
                },
                None,
            );
        }
        Some(crate::memory_cap::sheddable::ShedReport {
            bytes_freed,
            actions: entries_evicted,
            more_available,
        })
    }

    fn shed_path(&self) -> crate::observability::shed::ShedPath {
        crate::observability::shed::ShedPath::StoreInsert
    }
}
