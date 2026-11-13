use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use thiserror::Error;
use tokio::sync::{RwLock, broadcast};

use crate::store::RequestStore;
use crate::store::models::{ProxyEvent, RequestId};

#[derive(Debug, Clone)]
pub struct InFlightRecord {
    pub session_id: String,
    pub started_at: Instant,
    pub event_tx: broadcast::Sender<ProxyEvent>,
}

impl InFlightRecord {
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn elapsed(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }

    pub fn event_tx(&self) -> &broadcast::Sender<ProxyEvent> {
        &self.event_tx
    }
}

#[derive(Debug, Error)]
#[error("in-flight registry at capacity: {current}/{max}")]
pub struct CapacityError {
    pub current: usize,
    pub max: usize,
}

#[derive(Debug, Clone, Default)]
pub struct InFlightRegistry {
    inner: Arc<DashMap<RequestId, InFlightRecord>>,
    max: usize,
}

impl InFlightRegistry {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(max: usize) -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
            max,
        }
    }

    pub fn register(&self, id: RequestId, record: InFlightRecord) -> Result<(), CapacityError> {
        if self.max > 0 {
            let current = self.inner.len();
            if current >= self.max {
                return Err(CapacityError {
                    current,
                    max: self.max,
                });
            }
        }
        self.inner.insert(id, record);
        Ok(())
    }

    pub fn get(&self, id: RequestId) -> Option<InFlightRecord> {
        self.inner.get(&id).map(|r| r.value().clone())
    }

    pub fn remove(&self, id: RequestId) {
        self.inner.remove(&id);
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[cfg(test)]
    pub fn snapshot_ids(&self) -> Vec<RequestId> {
        self.inner.iter().map(|r| *r.key()).collect()
    }

    pub fn busy_session_ids(&self) -> std::collections::HashSet<String> {
        self.inner
            .iter()
            .map(|r| r.value().session_id.clone())
            .collect()
    }

    pub fn is_session_busy(&self, session_uuid: &str) -> bool {
        self.inner
            .iter()
            .any(|r| r.value().session_id == session_uuid)
    }
}

pub struct InFlightGuard {
    registry: Arc<InFlightRegistry>,
    store: Arc<RwLock<RequestStore>>,
    request_id: RequestId,
    armed: bool,
}

impl InFlightGuard {
    pub fn new(
        registry: Arc<InFlightRegistry>,
        store: Arc<RwLock<RequestStore>>,
        request_id: RequestId,
    ) -> Self {
        Self {
            registry,
            store,
            request_id,
            armed: true,
        }
    }

    pub fn forget(mut self) {
        self.armed = false;
    }
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let registry = self.registry.clone();
        let store = self.store.clone();
        let request_id = self.request_id;
        if tokio::runtime::Handle::try_current().is_err() {
            registry.remove(request_id);
            return;
        }
        tokio::spawn(async move {
            registry.remove(request_id);
            {
                let mut s = store.write().await;
                s.mark_aborted_if_non_terminal(
                    request_id,
                    "aborted: cleanup-on-drop before terminal event",
                );
            }
            crate::store::RequestStore::close_aborted_request_in_db(&store, request_id).await;
        });
    }
}

#[cfg(test)]
#[path = "in_flight_tests.rs"]
pub mod in_flight_tests;
