use arc_swap::ArcSwapOption;
use chrono::{DateTime, Utc};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiveQueueReading {
    pub(crate) at: DateTime<Utc>,
    pub(crate) owed_closed_at: Vec<DateTime<Utc>>,
    pub(crate) saturated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StaleBacklog {
    pub(crate) count: i64,
    pub(crate) oldest_closed_at: Option<DateTime<Utc>>,
    pub(crate) at_least: bool,
}

impl LiveQueueReading {
    pub(crate) fn from_claim(
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

    pub(crate) fn stale(&self, closed_before: DateTime<Utc>) -> StaleBacklog {
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

    pub(crate) fn age_secs(&self, now: DateTime<Utc>) -> i64 {
        (now - self.at).num_seconds().max(0)
    }
}

static LAST_CLAIM: ArcSwapOption<LiveQueueReading> = ArcSwapOption::const_empty();

pub(crate) fn publish(reading: LiveQueueReading) {
    LAST_CLAIM.store(Some(Arc::new(reading)));
}

pub(crate) fn current() -> Option<Arc<LiveQueueReading>> {
    LAST_CLAIM.load_full()
}
