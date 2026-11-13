use chrono::{DateTime, Utc};

use super::RequestStore;

impl RequestStore {
    pub fn observe_read_dedup(
        &mut self,
        session_id: &str,
        body: &serde_json::Value,
        now: DateTime<Utc>,
    ) -> Vec<crate::wire::read_dedup::DedupHit> {
        let index = self
            .session_dedup_index
            .entry(session_id.to_owned())
            .or_default();
        let stats = self
            .session_dedup_stats
            .entry(session_id.to_owned())
            .or_default();
        crate::wire::read_dedup::observe(body, index, stats, now)
    }

    pub fn rewrite_read_dedup(
        &mut self,
        session_id: &str,
        body: &serde_json::Value,
        now: DateTime<Utc>,
    ) -> (
        Vec<crate::wire::read_dedup::DedupHit>,
        Option<serde_json::Value>,
    ) {
        let index = self
            .session_dedup_index
            .entry(session_id.to_owned())
            .or_default();
        let stats = self
            .session_dedup_stats
            .entry(session_id.to_owned())
            .or_default();
        crate::wire::read_dedup::rewrite(body, index, stats, now)
    }

    pub fn dedup_stats(&self, session_id: &str) -> crate::wire::read_dedup::SessionDedupStats {
        self.session_dedup_stats
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }
}
