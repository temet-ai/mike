use std::sync::Arc;

use chrono::{Duration, Utc};

use super::open_memo::Watermark;
use crate::change_record::{OpenChange, WorkingCheckout};
use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn open_changes(&self, checkout: &str) -> Result<Vec<OpenChange>, PersistError> {
        let revision = self.memo::<super::open_memo::OpenChangesMemo>().revision();
        let newest = self.newest_change_event(checkout).await?;
        let at = Watermark { newest, revision };
        if let Some(rows) = self
            .memo::<super::open_memo::OpenChangesMemo>()
            .get(checkout, at)
        {
            return Ok(rows.as_ref().clone());
        }
        let rows = Arc::new(self.compute_open_changes(checkout).await?);
        self.memo::<super::open_memo::OpenChangesMemo>()
            .remember(checkout, at, Arc::clone(&rows));
        Ok(Arc::unwrap_or_clone(rows))
    }

    async fn newest_change_event(&self, checkout: &str) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db("change_record_newest_event", async move {
            let newest = sqlx::query_file_scalar!(
                "src/store/persistence/change_record/newest_change_event.sql",
                checkout,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(newest)
        })
        .await
    }

    async fn compute_open_changes(&self, checkout: &str) -> Result<Vec<OpenChange>, PersistError> {
        crate::metrics::time_db("change_record_open_changes", async move {
            let rows = sqlx::query_file_as!(
                OpenChange,
                "src/store/persistence/change_record/open_changes.sql",
                checkout,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn checkouts_with_working_sessions(
        &self,
        within: Duration,
    ) -> Result<Vec<WorkingCheckout>, PersistError> {
        let since = Utc::now() - within;
        crate::metrics::time_db("change_record_working_checkouts", async move {
            let rows = sqlx::query_file_as!(
                WorkingCheckout,
                "src/store/persistence/change_record/checkouts_with_working_sessions.sql",
                since,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
