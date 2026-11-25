use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use uuid::Uuid;

const PER_SESSION: usize = 500;
pub const QUIET_FOR: Duration = Duration::from_secs(20);
pub const BETWEEN_NOTES: Duration = Duration::from_secs(300);
const SENT_LIMIT: usize = 1024;

pub const VIA_REQUEST: &str = "request";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeNote {
    pub id: i64,
    pub path: String,
    pub line: i32,
    pub rule_id: String,
    pub message: String,
    pub fix: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delivered {
    pub finding_id: i64,
    pub request_id: Option<i64>,
    pub via: &'static str,
}

#[derive(Debug)]
struct Waiting {
    note: CodeNote,
    posted: Instant,
}

#[derive(Debug, Default)]
pub struct CodeNotes {
    waiting: Mutex<HashMap<Uuid, Vec<Waiting>>>,
    sent: Mutex<HashMap<u64, Vec<i64>>>,
    given: Mutex<HashMap<Uuid, Instant>>,
    delivered: Mutex<Vec<Delivered>>,
}

impl CodeNotes {
    pub fn post(&self, session: Uuid, notes: Vec<CodeNote>) {
        let Ok(mut waiting) = self.waiting.lock() else {
            return;
        };
        let queue = waiting.entry(session).or_default();
        for note in notes {
            if queue.len() < PER_SESSION && !queue.iter().any(|w| w.note.id == note.id) {
                queue.push(Waiting {
                    note,
                    posted: Instant::now(),
                });
            }
        }
    }

    pub fn withdraw(&self, ids: &[i64]) {
        if ids.is_empty() {
            return;
        }
        if let Ok(mut waiting) = self.waiting.lock() {
            for queue in waiting.values_mut() {
                queue.retain(|w| !ids.contains(&w.note.id));
            }
            waiting.retain(|_, queue| !queue.is_empty());
        }
    }

    pub fn take(&self, session: Uuid) -> Vec<CodeNote> {
        self.take_settled(session, QUIET_FOR, BETWEEN_NOTES)
    }

    pub fn take_settled(&self, session: Uuid, quiet: Duration, between: Duration) -> Vec<CodeNote> {
        let (Ok(mut waiting), Ok(mut given)) = (self.waiting.lock(), self.given.lock()) else {
            return Vec::new();
        };
        let settled = waiting
            .get(&session)
            .is_some_and(|queue| queue.iter().all(|w| w.posted.elapsed() >= quiet));
        let recent = given.get(&session).is_some_and(|at| at.elapsed() < between);
        if !settled || recent {
            return Vec::new();
        }
        given.retain(|_, at| at.elapsed() < between);
        given.insert(session, Instant::now());
        waiting
            .remove(&session)
            .unwrap_or_default()
            .into_iter()
            .map(|w| w.note)
            .collect()
    }

    pub fn put_back(&self, session: Uuid, notes: Vec<CodeNote>) {
        if let Ok(mut given) = self.given.lock() {
            given.remove(&session);
        }
        self.post(session, notes);
    }

    pub fn mark_sent(&self, ids: &[i64], pipeline_request: u64) {
        if let Ok(mut sent) = self.sent.lock() {
            if sent.len() >= SENT_LIMIT {
                sent.clear();
            }
            sent.insert(pipeline_request, ids.to_vec());
        }
    }

    pub fn bind_capture_id(&self, pipeline_request: u64, capture_request_id: i64) {
        let Some(ids) = self
            .sent
            .lock()
            .ok()
            .and_then(|mut sent| sent.remove(&pipeline_request))
        else {
            return;
        };
        self.deliver(&ids, Some(capture_request_id), VIA_REQUEST);
    }

    fn deliver(&self, ids: &[i64], request_id: Option<i64>, via: &'static str) {
        if let Ok(mut delivered) = self.delivered.lock() {
            delivered.extend(ids.iter().map(|finding_id| Delivered {
                finding_id: *finding_id,
                request_id,
                via,
            }));
        }
    }

    pub fn drain_delivered(&self) -> Vec<Delivered> {
        self.delivered
            .lock()
            .map(|mut delivered| std::mem::take(&mut *delivered))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests;
