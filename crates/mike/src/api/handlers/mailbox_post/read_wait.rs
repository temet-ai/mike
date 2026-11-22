use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;
use tokio::sync::watch;
use uuid::Uuid;

use crate::state::AppState;

const READ_WAIT: Duration = Duration::from_secs(2);

pub(super) struct Pending(watch::Sender<HashMap<Uuid, usize>>);

impl Default for Pending {
    fn default() -> Self {
        Self(watch::Sender::new(HashMap::new()))
    }
}

impl Pending {
    pub(super) async fn settled(&self, key: Uuid, at_most: Duration) {
        let mut counts = self.0.subscribe();
        let _ = tokio::time::timeout(
            at_most,
            counts.wait_for(|counts| !counts.contains_key(&key)),
        )
        .await;
    }

    fn add(&self, keys: &[Uuid]) {
        self.0.send_modify(|counts| {
            for key in keys {
                *counts.entry(*key).or_default() += 1;
            }
        });
    }

    fn remove(&self, keys: &[Uuid]) {
        self.0.send_modify(|counts| {
            for key in keys {
                if let Some(count) = counts.get_mut(key) {
                    *count -= 1;
                    if *count == 0 {
                        counts.remove(key);
                    }
                }
            }
        });
    }
}

pub(super) struct Mark {
    pending: Arc<Pending>,
    keys: Vec<Uuid>,
    released: AtomicBool,
}

impl Mark {
    pub(super) fn new(pending: Arc<Pending>, keys: Vec<Uuid>) -> Arc<Self> {
        pending.add(&keys);
        Arc::new(Self {
            pending,
            keys,
            released: AtomicBool::new(false),
        })
    }

    pub(super) fn release(&self) {
        if !self.released.swap(true, Ordering::AcqRel) {
            self.pending.remove(&self.keys);
        }
    }
}

impl Drop for Mark {
    fn drop(&mut self) {
        self.release();
    }
}

pub(crate) async fn reads_after_sends(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if let (Some(handle), Some(key)) = (state.persist(), mailbox_read_key(&request)) {
        super::writer::pending(handle).settled(key, READ_WAIT).await;
    }
    next.run(request).await
}

fn mailbox_read_key(request: &Request) -> Option<Uuid> {
    if request.method() != Method::GET {
        return None;
    }
    read_key(request.uri().path())
}

fn read_key(path: &str) -> Option<Uuid> {
    let mut parts = path.trim_end_matches('/').split('/').skip(1);
    if parts.next()? != "api" {
        return None;
    }
    match (parts.next()?, parts.next()?, parts.next()) {
        ("sessions", id, Some("messages" | "threads")) => id.parse().ok(),
        ("threads", id, None) => id.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
#[path = "read_wait_tests.rs"]
mod tests;
