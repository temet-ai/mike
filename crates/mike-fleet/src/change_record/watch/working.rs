use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::change_feed::{ChangeFeed, DirtySessions, Generation};
use crate::change_record::WorkingCheckout;
use crate::store::models::RowChangedTable;

pub(super) const WAKES: [RowChangedTable; 4] = [
    RowChangedTable::Requests,
    RowChangedTable::Locations,
    RowChangedTable::CheckoutEdits,
    RowChangedTable::ChangeGenerations,
];
const PLACEMENTS: [RowChangedTable; 2] =
    [RowChangedTable::Locations, RowChangedTable::CheckoutEdits];
const SESSION_PROJECTS: &str = "session_projects";

pub(super) fn worked_within() -> chrono::Duration {
    chrono::Duration::minutes(30)
}

pub(super) struct Working {
    feed: ChangeFeed,
    requests: DirtySessions,
    known: HashSet<String>,
    stamp: Option<Vec<Generation>>,
    expires: Option<DateTime<Utc>>,
}

pub(super) struct Stamp(Vec<Generation>);

impl Working {
    pub(super) fn new(feed: ChangeFeed) -> Self {
        let requests = feed.subscribe_sessions(&[RowChangedTable::Requests]);
        Self {
            feed,
            requests,
            known: HashSet::new(),
            stamp: None,
            expires: None,
        }
    }

    pub(super) fn feed(&self) -> &ChangeFeed {
        &self.feed
    }

    fn placements(&self) -> Vec<Generation> {
        self.feed.generations(
            &PLACEMENTS,
            &[RowChangedTable::ChangeGenerations],
            &[SESSION_PROJECTS.to_owned()],
        )
    }

    pub(super) fn due(&mut self, now: DateTime<Utc>) -> bool {
        let drained = self.requests.drain();
        let unknown = drained
            .sessions
            .iter()
            .any(|session| !self.known.contains(session));
        let moved = self.stamp.as_ref() != Some(&self.placements());
        let expired = self.expires.is_some_and(|at| now >= at);
        !self.feed.is_fed() || drained.everything || unknown || moved || expired
    }

    pub(super) fn before_read(&self) -> Stamp {
        Stamp(self.placements())
    }

    pub(super) fn read(
        &mut self,
        stamp: Stamp,
        rows: &[WorkingCheckout],
        expires: Option<DateTime<Utc>>,
    ) {
        self.known = rows
            .iter()
            .flat_map(|row| row.sessions.iter().map(ToString::to_string))
            .collect();
        self.stamp = Some(stamp.0);
        self.expires = expires;
    }

    pub(super) fn forget(&mut self) {
        self.stamp = None;
        self.expires = None;
    }

    pub(super) fn expires(&self) -> Option<DateTime<Utc>> {
        self.expires
    }

    pub(super) fn wait(&self, now: DateTime<Utc>, cap: Duration) -> Duration {
        self.expires.map_or(cap, |at| {
            (at - now).to_std().unwrap_or(Duration::ZERO).min(cap)
        })
    }
}

pub(super) fn expiry(newest: &BTreeMap<String, DateTime<Utc>>) -> Option<DateTime<Utc>> {
    newest
        .values()
        .min()
        .map(|oldest| *oldest + worked_within() + chrono::Duration::seconds(1))
}
