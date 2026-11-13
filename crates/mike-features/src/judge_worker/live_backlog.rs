use arc_swap::ArcSwapOption;
use chrono::{DateTime, Utc};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveQueueReading {
    pub at: DateTime<Utc>,
    pub owed_closed_at: Vec<DateTime<Utc>>,
    pub saturated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaleBacklog {
    pub count: i64,
    pub oldest_closed_at: Option<DateTime<Utc>>,
    pub at_least: bool,
}

impl LiveQueueReading {
    pub fn from_claim(
        claimed: &[(i64, DateTime<Utc>)],
        limit: i64,
        at: DateTime<Utc>,
    ) -> Self {
        let mut owed_closed_at: Vec<DateTime<Utc>> =
            claimed.iter().map(|(_, closed_at)| *closed_at).collect();
        owed_closed_at.sort_unstable();
        let claimed_count = i64::try_from(owed_closed_at.len()).unwrap_or(i64::MAX);
        Self {
            at,
            saturated: limit > 0 && claimed_count >= limit,
            owed_closed_at,
        }
    }

    pub fn stale(&self, closed_before: DateTime<Utc>) -> StaleBacklog {
        let stale = self
            .owed_closed_at
            .iter()
            .take_while(|closed_at| **closed_at < closed_before)
            .count();
        StaleBacklog {
            count: i64::try_from(stale).unwrap_or(i64::MAX),
            oldest_closed_at: self.owed_closed_at.first().copied().filter(|_| stale > 0),
            at_least: self.saturated && stale > 0 && stale == self.owed_closed_at.len(),
        }
    }

    pub fn age_secs(&self, now: DateTime<Utc>) -> i64 {
        (now - self.at).num_seconds().max(0)
    }
}

static LAST_CLAIM: ArcSwapOption<LiveQueueReading> = ArcSwapOption::const_empty();

pub fn publish(reading: LiveQueueReading) {
    LAST_CLAIM.store(Some(Arc::new(reading)));
}

pub fn current() -> Option<Arc<LiveQueueReading>> {
    LAST_CLAIM.load_full()
}
