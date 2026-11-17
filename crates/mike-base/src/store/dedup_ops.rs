use chrono::{DateTime, Utc};

use super::RequestStore;
use crate::wire::read_dedup::{
    DedupHit, DedupInput, SessionDedupIndex, SessionDedupStats, observe, rewrite,
};

impl RequestStore {
    pub fn observe_read_dedup(
        &mut self,
        session_id: &str,
        input: DedupInput<'_>,
        now: DateTime<Utc>,
    ) -> Vec<DedupHit> {
        let (index, stats) = self.dedup_state(session_id);
        observe(input, index, stats, now)
    }

    pub fn rewrite_read_dedup(
        &mut self,
        session_id: &str,
        input: DedupInput<'_>,
        now: DateTime<Utc>,
    ) -> (Vec<DedupHit>, Option<serde_json::Value>) {
        let (index, stats) = self.dedup_state(session_id);
        rewrite(input, index, stats, now)
    }

    fn dedup_state(
        &mut self,
        session_id: &str,
    ) -> (&mut SessionDedupIndex, &mut SessionDedupStats) {
        let index = self
            .session_dedup_index
            .entry(session_id.to_owned())
            .or_default();
        let stats = self
            .session_dedup_stats
            .entry(session_id.to_owned())
            .or_default();
        (index, stats)
    }

    pub fn dedup_stats(&self, session_id: &str) -> SessionDedupStats {
        self.session_dedup_stats
            .get(session_id)
            .cloned()
            .unwrap_or_default()
    }
}
