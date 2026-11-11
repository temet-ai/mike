use super::open_shown::quoted_pairs;
use crate::store::persistence::{PersistError, PersistHandle};

pub(crate) const USED_BATCH: i64 = 250;
pub(crate) const WINDOW_TURNS: i64 = 6;
pub(crate) const MAX_OPEN_SECS: f64 = 86_400.0;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UsedPass {
    pub(crate) updated: u64,
    pub(crate) with_used: u64,
    pub(crate) finalised: u64,
}

impl PersistHandle {
    pub(crate) async fn mark_used_search_references(
        &self,
        batch: i64,
    ) -> Result<UsedPass, PersistError> {
        self.mark_used_within(batch, WINDOW_TURNS, MAX_OPEN_SECS)
            .await
    }

    pub(super) async fn mark_used_within(
        &self,
        batch: i64,
        window_turns: i64,
        max_open_secs: f64,
    ) -> Result<UsedPass, PersistError> {
        let quoted = quoted_pairs(self.open_shown_rows(batch, window_turns).await?);
        let rows = sqlx::query_file!(
            "src/store/persistence/search_events/mark_used.sql",
            batch,
            window_turns,
            max_open_secs,
            &quoted.event_ids,
            &quoted.references,
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .fold(UsedPass::default(), |pass, row| UsedPass {
                updated: pass.updated + 1,
                with_used: pass.with_used + u64::from(row.used > 0),
                finalised: pass.finalised + u64::from(row.finalised),
            }))
    }
}
