//! The newest change event this process wrote for each checkout. The code
//! scan reads new events for the checkouts this machine watches, and this
//! machine's watcher is what writes them, so the scan can see from memory
//! whether anything arrived since its cursor.

use dashmap::DashMap;

const CAPACITY: usize = 256;

#[derive(Default)]
struct NewestEvents(DashMap<String, i64>);

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub fn note_change_events(self, checkouts: &[String], ids: &[i64]) {
        let newest = self.memo::<NewestEvents>();
        if newest.0.len() >= CAPACITY {
            newest.0.clear();
        }
        for (checkout, id) in checkouts.iter().zip(ids) {
            newest
                .0
                .entry(checkout.clone())
                .and_modify(|seen| *seen = (*seen).max(*id))
                .or_insert(*id);
        }
    }

    /// The newest event id written for `checkout` since this process started.
    pub fn newest_written_change_event(self, checkout: &str) -> Option<i64> {
        self.memo::<NewestEvents>().0.get(checkout).map(|id| *id)
    }
}
