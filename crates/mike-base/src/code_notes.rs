use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

const PER_SESSION: usize = 20;

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
    pub request_id: i64,
}

#[derive(Debug, Default)]
pub struct CodeNotes {
    waiting: Mutex<HashMap<Uuid, Vec<CodeNote>>>,
    delivered: Mutex<Vec<Delivered>>,
}

impl CodeNotes {
    pub fn post(&self, session: Uuid, notes: Vec<CodeNote>) {
        let Ok(mut waiting) = self.waiting.lock() else {
            return;
        };
        let queue = waiting.entry(session).or_default();
        for note in notes {
            if queue.len() < PER_SESSION && !queue.iter().any(|n| n.id == note.id) {
                queue.push(note);
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
        let taken: Vec<CodeNote> = queue.drain(..max.min(queue.len())).collect();
        if queue.is_empty() {
            waiting.remove(&session);
        }
        taken
    }

    pub fn put_back(&self, session: Uuid, notes: Vec<CodeNote>) {
        self.post(session, notes);
    }

    pub fn mark_delivered(&self, ids: &[i64], request_id: i64) {
        if let Ok(mut delivered) = self.delivered.lock() {
            delivered.extend(ids.iter().map(|finding_id| Delivered {
                finding_id: *finding_id,
                request_id,
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
mod tests {
    use super::{CodeNote, CodeNotes};
    use uuid::Uuid;

    fn note(id: i64) -> CodeNote {
        CodeNote {
            id,
            path: "a.rs".to_owned(),
            line: 1,
            rule_id: "r".to_owned(),
            message: "m".to_owned(),
            fix: None,
        }
    }

    #[test]
    fn notes_are_taken_once_per_session_in_order_without_duplicates() {
        let notes = CodeNotes::default();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        notes.post(a, vec![note(1), note(2), note(1)]);
        notes.post(a, vec![note(3)]);
        notes.post(b, vec![note(9)]);
        let first: Vec<i64> = notes.take(a, 2).iter().map(|n| n.id).collect();
        assert_eq!(first, vec![1, 2]);
        assert_eq!(notes.take(a, 3).len(), 1);
        assert!(notes.take(a, 3).is_empty());
        assert_eq!(notes.take(b, 3).len(), 1);
    }

    #[test]
    fn delivered_ids_are_drained_once() {
        let notes = CodeNotes::default();
        notes.mark_delivered(&[4, 5], 77);
        assert_eq!(notes.drain_delivered().len(), 2);
        assert!(notes.drain_delivered().is_empty());
    }
}
