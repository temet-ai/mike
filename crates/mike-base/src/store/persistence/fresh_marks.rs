use chrono::{DateTime, Utc};
use dashmap::DashMap;

use super::PersistHandle;
use crate::change_feed::Generation;
use crate::store::models::RowChangedTable;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreshTicket(Generation);

#[derive(Debug, Default)]
pub struct FreshMarks {
    marks: DashMap<&'static str, (Generation, Option<DateTime<Utc>>)>,
}

impl PersistHandle {
    pub fn fresh_ticket(&self) -> Option<FreshTicket> {
        self.feed_memos
            .fed_feed()
            .map(|feed| FreshTicket(feed.generation()))
    }

    pub fn is_fresh(
        &self,
        key: &'static str,
        tables: &[RowChangedTable],
        now: DateTime<Utc>,
    ) -> bool {
        let Some(feed) = self.feed_memos.fed_feed() else {
            return false;
        };
        let Some(mark) = self.feed_memos.fresh().marks.get(key).map(|m| *m) else {
            return false;
        };
        let (at, until) = mark;
        until.is_none_or(|until| now < until) && !feed.changed_since(tables, at)
    }

    pub fn mark_fresh(
        &self,
        key: &'static str,
        ticket: Option<FreshTicket>,
        until: Option<DateTime<Utc>>,
    ) {
        if let Some(FreshTicket(at)) = ticket {
            self.feed_memos.fresh().marks.insert(key, (at, until));
        }
    }

    pub fn forget_fresh(&self, key: &'static str) {
        self.feed_memos.fresh().marks.remove(key);
    }
}

#[cfg(test)]
#[path = "fresh_marks_tests.rs"]
mod fresh_marks_tests;
