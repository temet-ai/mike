use uuid::Uuid;

use super::GuardSteererRow;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistError;
use crate::store::persistence::feed_memo::{Memo, Scope};

const ANY_SESSION: Uuid = Uuid::nil();

struct GuardSteererMemo(Memo<Option<GuardSteererRow>>);

impl Default for GuardSteererMemo {
    fn default() -> Self {
        Self(Memo::new(Scope {
            tables: &[RowChangedTable::Steerers],
            per_session: &[],
        }))
    }
}

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub async fn guard_steerer(self) -> Result<Option<GuardSteererRow>, PersistError> {
        let held = self.memo::<GuardSteererMemo>();
        let memo = self.feed_memos.bound(&held.0);
        if let Some(row) = memo.as_ref().and_then(|memo| memo.get(ANY_SESSION)) {
            return Ok(row);
        }
        let ticket = memo.as_ref().map(|memo| memo.ticket());
        let row = self.read_guard_steerer().await?;
        if let (Some(memo), Some(ticket)) = (memo, ticket) {
            memo.remember(ANY_SESSION, ticket, row.clone());
        }
        Ok(row)
    }

    async fn read_guard_steerer(self) -> Result<Option<GuardSteererRow>, PersistError> {
        crate::metrics::time_db("guard_steerer", async move {
            Ok(sqlx::query_as!(
                GuardSteererRow,
                "SELECT id, enabled, config FROM steering.steerers
                  WHERE name = 'turn-guard' AND deleted_at IS NULL ORDER BY id LIMIT 1",
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }
}

#[cfg(test)]
#[path = "steerer_row_tests.rs"]
mod steerer_row_tests;
