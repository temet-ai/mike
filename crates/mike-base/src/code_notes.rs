use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use uuid::Uuid;

const PER_SESSION: usize = 20;
const SENT_LIMIT: usize = 1024;
const PUSHED_UNBOUND_AFTER: Duration = Duration::from_secs(600);

pub const VIA_REQUEST: &str = "request";
pub const VIA_PUSH: &str = "push";

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
    pushable: bool,
}

#[derive(Debug)]
struct Pushed {
    at: Instant,
    ids: Vec<i64>,
}

#[derive(Debug, Default)]
pub struct CodeNotes {
    waiting: Mutex<HashMap<Uuid, Vec<Waiting>>>,
    sent: Mutex<HashMap<u64, Vec<i64>>>,
    pushed: Mutex<HashMap<Uuid, Vec<Pushed>>>,
    delivered: Mutex<Vec<Delivered>>,
}

impl CodeNotes {
    pub fn post(&self, session: Uuid, notes: Vec<CodeNote>) {
        self.queue(session, notes, true);
    }

    fn queue(&self, session: Uuid, notes: Vec<CodeNote>, pushable: bool) {
        let Ok(mut waiting) = self.waiting.lock() else {
            return;
        };
        let queue = waiting.entry(session).or_default();
        for note in notes {
            if queue.len() < PER_SESSION && !queue.iter().any(|w| w.note.id == note.id) {
                queue.push(Waiting {
                    note,
                    posted: Instant::now(),
                    pushable,
                });
            }
        }
    }

    pub fn take(&self, session: Uuid, max: usize) -> Vec<CodeNote> {
        let Ok(mut waiting) = self.waiting.lock() else {
            return Vec::new();
        };
        let Some(queue) = waiting.get_mut(&session) else {
            return Vec::new();
        };
        let taken: Vec<CodeNote> = queue
            .drain(..max.min(queue.len()))
            .map(|w| w.note)
            .collect();
        if queue.is_empty() {
            waiting.remove(&session);
        }
        taken
    }

    pub fn take_unread(&self, older_than: Duration, max: usize) -> Vec<(Uuid, Vec<CodeNote>)> {
        let Ok(mut waiting) = self.waiting.lock() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (session, queue) in waiting.iter_mut() {
            let ready = queue
                .iter()
                .any(|w| w.pushable && w.posted.elapsed() >= older_than);
            if !ready {
                continue;
            }
            let taken: Vec<CodeNote> = queue
                .drain(..max.min(queue.len()))
                .map(|w| w.note)
                .collect();
            out.push((*session, taken));
        }
        waiting.retain(|_, queue| !queue.is_empty());
        out
    }

    pub fn put_back(&self, session: Uuid, notes: Vec<CodeNote>) {
        self.queue(session, notes, true);
    }

    pub fn put_back_unpushable(&self, session: Uuid, notes: Vec<CodeNote>) {
        self.queue(session, notes, false);
    }

    pub fn mark_sent(&self, ids: &[i64], pipeline_request: u64) {
        if let Ok(mut sent) = self.sent.lock() {
            if sent.len() >= SENT_LIMIT {
                sent.clear();
            }
            sent.insert(pipeline_request, ids.to_vec());
        }
    }

    pub fn mark_pushed(&self, session: Uuid, ids: &[i64]) {
        if let Ok(mut pushed) = self.pushed.lock() {
            pushed.entry(session).or_default().push(Pushed {
                at: Instant::now(),
                ids: ids.to_vec(),
            });
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

    pub fn bind_session_request(&self, session: Uuid, capture_request_id: i64) {
        let Some(batches) = self
            .pushed
            .lock()
            .ok()
            .and_then(|mut pushed| pushed.remove(&session))
        else {
            return;
        };
        for batch in batches {
            self.deliver(&batch.ids, Some(capture_request_id), VIA_PUSH);
        }
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

    fn expire_pushed(&self) {
        let expired: Vec<i64> = match self.pushed.lock() {
            Ok(mut pushed) => {
                let mut ids = Vec::new();
                for batches in pushed.values_mut() {
                    batches.retain(|batch| {
                        let keep = batch.at.elapsed() < PUSHED_UNBOUND_AFTER;
                        if !keep {
                            ids.extend(batch.ids.iter().copied());
                        }
                        keep
                    });
                }
                pushed.retain(|_, batches| !batches.is_empty());
                ids
            }
            Err(_) => return,
        };
        self.deliver(&expired, None, VIA_PUSH);
    }

    pub fn drain_delivered(&self) -> Vec<Delivered> {
        self.expire_pushed();
        self.delivered
            .lock()
            .map(|mut delivered| std::mem::take(&mut *delivered))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests;
