use uuid::Uuid;

use super::feed_memo::{Memo, Scope};
use super::location_lifecycle::CheckoutRow;
use super::project_placement::{MachineLocation, MachineLocationRoot};
use super::{PersistError, PersistHandle};
use crate::store::models::RowChangedTable;

const LOCATIONS: Scope = Scope {
    tables: &[RowChangedTable::Locations],
    per_session: &[],
};

struct MachineCheckouts(Memo<Vec<CheckoutRow>>);

impl Default for MachineCheckouts {
    fn default() -> Self {
        Self(Memo::new(LOCATIONS))
    }
}

impl PersistHandle {
    pub async fn checkout_rows(&self, machine: Uuid) -> Result<Vec<CheckoutRow>, PersistError> {
        let slot = self.memo::<MachineCheckouts>();
        let bound = self
            .feed_memos
            .fed_feed()
            .and(self.feed_memos.bound(&slot.0));
        if let Some(kept) = bound.as_ref().and_then(|memo| memo.get(machine)) {
            return Ok(kept);
        }
        let ticket = bound.as_ref().map(|memo| memo.ticket());
        let rows = self.read_checkout_rows(machine).await?;
        if let (Some(memo), Some(ticket)) = (bound, ticket) {
            memo.remember(machine, ticket, rows.clone());
        }
        Ok(rows)
    }

    pub async fn locations_on_machine(
        &self,
        machine: Uuid,
    ) -> Result<Vec<MachineLocation>, PersistError> {
        Ok(self
            .checkout_rows(machine)
            .await?
            .into_iter()
            .map(|row| MachineLocation {
                location_uuid: row.location_uuid,
                root_path: row.root_path,
                origin_url: row.origin_url,
            })
            .collect())
    }

    pub async fn machine_location_roots(
        &self,
        machine: Uuid,
    ) -> Result<Vec<MachineLocationRoot>, PersistError> {
        Ok(self
            .checkout_rows(machine)
            .await?
            .into_iter()
            .map(|row| MachineLocationRoot {
                root_path: row.root_path,
                project_id: row.project_id,
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "location_memo_tests.rs"]
mod location_memo_tests;

#[cfg(test)]
#[path = "locations_row_changed_tests.rs"]
mod locations_row_changed_tests;
