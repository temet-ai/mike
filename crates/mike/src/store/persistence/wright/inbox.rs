use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::store::persistence::PersistError;

const UNREAD: &str = include_str!("sql/inbox/unread.sql");
const MARK_READ: &str = include_str!("sql/inbox/mark_read.sql");
pub(crate) const INBOX_LIMIT: i64 = 50;

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub(crate) struct InboxRow {
    pub(crate) id: i64,
    pub(crate) kind: String,
    pub(crate) ref_id: i64,
    pub(crate) state: String,
    pub(crate) problem_id: Option<i64>,
    pub(crate) summary: String,
    pub(crate) created_at: DateTime<Utc>,
}

impl crate::store::persistence::web_repo::WebDb<'_> {
    pub(crate) async fn unread_wright_inbox(
        self,
        check_name: &str,
    ) -> Result<Vec<InboxRow>, PersistError> {
        Ok(
            sqlx::query_as(UNREAD) // grader-allow: Wright's store runs its include_str! SQL at runtime; no .sqlx entry
                .bind(check_name)
                .bind(INBOX_LIMIT)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn mark_wright_inbox_read(self, run_id: i64) -> Result<u64, PersistError> {
        Ok(
            sqlx::query(MARK_READ) // grader-allow: Wright's store runs its include_str! SQL at runtime; no .sqlx entry
                .bind(run_id)
                .execute(self.pool())
                .await?
                .rows_affected(),
        )
    }
}
