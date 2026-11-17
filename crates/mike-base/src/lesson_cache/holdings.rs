use std::collections::{HashMap, HashSet};
use std::sync::{PoisonError, RwLock};

#[cfg(test)]
#[path = "holdings_tests.rs"]
mod holdings_tests;

const SESSION_CAPACITY: usize = 16_384;

#[derive(Debug, Default)]
pub struct SessionHoldings {
    clients: RwLock<HashSet<String>>,
    held: RwLock<HashMap<String, HashMap<i64, i32>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldRow {
    pub session: String,
    pub lesson_id: i64,
    pub revision: i32,
}

impl SessionHoldings {
    pub fn note_client(&self, session: &str) {
        let mut clients = self.clients.write().unwrap_or_else(PoisonError::into_inner);
        if clients.contains(session) {
            return;
        }
        if clients.len() >= SESSION_CAPACITY
            && let Some(evicted) = clients.iter().next().cloned()
        {
            clients.remove(&evicted);
        }
        clients.insert(session.to_owned());
    }

    pub fn is_client(&self, session: &str) -> bool {
        self.clients
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(session)
    }

    pub fn holds(&self, session: &str, lesson_id: i64, revision: i32) -> bool {
        self.held
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(session)
            .and_then(|lessons| lessons.get(&lesson_id))
            .is_some_and(|held| *held == revision)
    }

    pub fn held_ids(&self, session: &str) -> Vec<i64> {
        let held = self.held.read().unwrap_or_else(PoisonError::into_inner);
        let mut ids: Vec<i64> = held
            .get(session)
            .map(|lessons| lessons.keys().copied().collect())
            .unwrap_or_default();
        ids.sort_unstable();
        ids
    }

    pub fn hold(&self, session: &str, lessons: &[(i64, i32)]) -> Vec<(i64, i32)> {
        if lessons.is_empty() {
            return Vec::new();
        }
        let mut held = self.held.write().unwrap_or_else(PoisonError::into_inner);
        if held.len() >= SESSION_CAPACITY
            && !held.contains_key(session)
            && let Some(evicted) = held.keys().next().cloned()
        {
            held.remove(&evicted);
        }
        let entry = held.entry(session.to_owned()).or_default();
        lessons
            .iter()
            .copied()
            .filter(|(id, revision)| entry.insert(*id, *revision) != Some(*revision))
            .collect()
    }

    pub fn forget(&self, session: &str) {
        self.held
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(session);
    }

    pub fn restore(&self, clients: Vec<String>, rows: Vec<HeldRow>) {
        {
            let mut known = self.clients.write().unwrap_or_else(PoisonError::into_inner);
            known.extend(clients);
        }
        let mut held = self.held.write().unwrap_or_else(PoisonError::into_inner);
        for row in rows {
            held.entry(row.session)
                .or_default()
                .entry(row.lesson_id)
                .or_insert(row.revision);
        }
    }
}
