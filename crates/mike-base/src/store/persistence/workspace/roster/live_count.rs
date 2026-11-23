use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::Serialize;
use uuid::Uuid;

use super::Assembled;
use super::assemble::earliest_flip;
use crate::change_feed::{ChangeFeed, Generation};
use crate::store::models::RowChangedTable;
use crate::store::persistence::{PersistError, PersistHandle};
use crate::workspace::{Liveness, RosterScope};

const TABLES: &[RowChangedTable] = &[
    RowChangedTable::WorkspaceSessionRegistrations,
    RowChangedTable::RemoteControlChannels,
    RowChangedTable::RuntimeSessionFacts,
];
const PER_SESSION: &[RowChangedTable] = &[RowChangedTable::Requests];
const CAPACITY: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LiveCount {
    pub live: usize,
}

#[derive(Clone)]
struct Held {
    live: Arc<Vec<Uuid>>,
    watched: Vec<String>,
    stamp: Vec<Generation>,
    until: Option<DateTime<Utc>>,
}

impl Held {
    fn from_assembled(assembled: &Assembled) -> Self {
        let live: Vec<Uuid> = assembled
            .rows
            .iter()
            .filter(|row| row.liveness == Liveness::Live)
            .map(|row| row.session_uuid)
            .collect();
        let watched = assembled
            .candidates
            .iter()
            .filter(|session| !live.contains(session))
            .map(Uuid::to_string)
            .collect();
        let until = assembled
            .rows
            .iter()
            .filter_map(|row| earliest_flip(row, &assembled.clock))
            .min();
        Self {
            live: Arc::new(live),
            watched,
            stamp: Vec::new(),
            until,
        }
    }

    fn generations(&self, feed: &ChangeFeed) -> Vec<Generation> {
        feed.generations(TABLES, PER_SESSION, &self.watched)
    }

    fn holds(&self, feed: &ChangeFeed, now: DateTime<Utc>) -> bool {
        self.until.is_none_or(|until| now < until) && self.generations(feed) == self.stamp
    }
}

#[derive(Default)]
struct LiveCounts(DashMap<i64, Held>);

impl PersistHandle {
    pub async fn count_live_peers(
        &self,
        project_id: i64,
        exclude: Option<Uuid>,
    ) -> Result<LiveCount, PersistError> {
        let live = self.live_members(project_id).await?;
        Ok(LiveCount {
            live: live
                .iter()
                .filter(|session| Some(**session) != exclude)
                .count(),
        })
    }

    async fn live_members(&self, project_id: i64) -> Result<Arc<Vec<Uuid>>, PersistError> {
        let feed = self.feed_memos.fed_feed();
        let counts = self.memo::<LiveCounts>();
        if let Some(feed) = feed
            && let Some(held) = counts.0.get(&project_id)
            && held.holds(feed, Utc::now())
        {
            return Ok(Arc::clone(&held.live));
        }
        let watermark = feed.map(ChangeFeed::generation);
        let assembled = self.assemble_roster(project_id, RosterScope::Live).await?;
        let mut held = Held::from_assembled(&assembled);
        let live = Arc::clone(&held.live);
        if let (Some(feed), Some(watermark)) = (feed, watermark) {
            held.stamp = held.generations(feed);
            if held.stamp.iter().all(|generation| *generation <= watermark) {
                if counts.0.len() >= CAPACITY && !counts.0.contains_key(&project_id) {
                    counts.0.clear();
                }
                counts.0.insert(project_id, held);
            } else {
                counts.0.remove(&project_id);
            }
        }
        Ok(live)
    }
}

#[cfg(test)]
#[path = "live_count_tests.rs"]
mod live_count_tests;
