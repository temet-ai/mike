use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;

use crate::change_record::OpenChange;

const CHECKOUT_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Watermark {
    pub newest: Option<i64>,
    pub revision: u64,
}

#[derive(Debug)]
struct Entry {
    at: Watermark,
    rows: Arc<Vec<OpenChange>>,
}

#[derive(Debug, Default)]
struct Inner {
    revision: AtomicU64,
    entries: DashMap<String, Entry>,
}

#[derive(Debug, Clone, Default)]
pub struct OpenChangesMemo {
    inner: Arc<Inner>,
}

impl OpenChangesMemo {
    pub fn revision(&self) -> u64 {
        self.inner.revision.load(Ordering::Acquire)
    }

    pub fn revise(&self) {
        self.inner.revision.fetch_add(1, Ordering::AcqRel);
    }

    pub fn get(
        &self,
        checkout: &str,
        at: Watermark,
    ) -> Option<Arc<Vec<OpenChange>>> {
        let entry = self.inner.entries.get(checkout)?;
        (entry.at == at).then(|| Arc::clone(&entry.rows))
    }

    pub fn remember(
        &self,
        checkout: &str,
        at: Watermark,
        rows: Arc<Vec<OpenChange>>,
    ) {
        if at.revision != self.revision() {
            return;
        }
        if self.inner.entries.len() >= CHECKOUT_CAPACITY
            && !self.inner.entries.contains_key(checkout)
        {
            self.inner.entries.clear();
        }
        self.inner
            .entries
            .insert(checkout.to_owned(), Entry { at, rows });
    }
}

#[cfg(test)]
#[path = "open_memo_tests.rs"]
pub mod open_memo_tests;
