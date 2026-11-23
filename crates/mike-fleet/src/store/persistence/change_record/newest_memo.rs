use dashmap::DashMap;

const CAPACITY: usize = 256;

#[derive(Default)]
struct NewestEvents(DashMap<String, i64>);

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub fn note_change_events(self, checkouts: &[Option<String>], ids: &[i64]) {
        let newest = self.memo::<NewestEvents>();
        if newest.0.len() >= CAPACITY {
            newest.0.clear();
        }
        for (checkout, id) in checkouts.iter().zip(ids) {
            let Some(checkout) = checkout else {
                continue;
            };
            newest
                .0
                .entry(checkout.clone())
                .and_modify(|seen| *seen = (*seen).max(*id))
                .or_insert(*id);
        }
    }

    pub fn newest_written_change_event(self, checkout: &str) -> Option<i64> {
        self.memo::<NewestEvents>().0.get(checkout).map(|id| *id)
    }
}
