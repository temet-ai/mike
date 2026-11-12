use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use thiserror::Error;
use tokio::sync::{RwLock, broadcast};

use crate::store::RequestStore;
use crate::store::models::{ProxyEvent, RequestId};

#[derive(Debug, Clone)]
pub struct InFlightRecord {
    pub(crate) session_id: String,
    pub(crate) started_at: Instant,
    pub(crate) event_tx: broadcast::Sender<ProxyEvent>,
}

impl InFlightRecord {
    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }

    pub(crate) fn elapsed(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }

    pub(crate) fn event_tx(&self) -> &broadcast::Sender<ProxyEvent> {
        &self.event_tx
    }
}

#[derive(Debug, Error)]
#[error("in-flight registry at capacity: {current}/{max}")]
pub(crate) struct CapacityError {
    pub(crate) current: usize,
    pub(crate) max: usize,
}

#[derive(Debug, Clone, Default)]
pub struct InFlightRegistry {
    inner: Arc<DashMap<RequestId, InFlightRecord>>,
    max: usize,
}

impl InFlightRegistry {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_capacity(max: usize) -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
            max,
        }
    }

    pub(crate) fn register(
        &self,
        id: RequestId,
        record: InFlightRecord,
    ) -> Result<(), CapacityError> {
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

    pub(crate) fn get(&self, id: RequestId) -> Option<InFlightRecord> {
        self.inner.get(&id).map(|r| r.value().clone())
    }

    pub(crate) fn remove(&self, id: RequestId) {
        self.inner.remove(&id);
    }

    pub(crate) fn len(&self) -> usize {
        self.inner.len()
    }

    #[cfg(test)]
    pub(crate) fn snapshot_ids(&self) -> Vec<RequestId> {
        self.inner.iter().map(|r| *r.key()).collect()
    }

    pub(crate) fn busy_session_ids(&self) -> std::collections::HashSet<String> {
        self.inner
            .iter()
            .map(|r| r.value().session_id.clone())
            .collect()
    }

    pub(crate) fn is_session_busy(&self, session_uuid: &str) -> bool {
        self.inner
            .iter()
            .any(|r| r.value().session_id == session_uuid)
    }
}

pub(crate) struct InFlightGuard {
    registry: Arc<InFlightRegistry>,
    store: Arc<RwLock<RequestStore>>,
    request_id: RequestId,
    armed: bool,
}

impl InFlightGuard {
    pub(crate) fn new(
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

    pub(crate) fn forget(mut self) {
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
mod in_flight_tests;
