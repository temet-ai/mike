use chrono::{DateTime, Utc};

use super::super::PersistError;
use super::{DossierEntryRow, EntryClose};

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct DirectiveSourceRow {
    pub turn_id: i64,
    pub session_id: i64,
    pub start_request_id: i64,
    pub user_prompt: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct PromiseSourceRow {
    pub turn_id: i64,
    pub session_id: i64,
    pub stop_request_id: Option<i64>,
    pub stop_completed_at: Option<DateTime<Utc>>,
    pub closed_at: DateTime<Utc>,
    pub final_text: String,
    pub text_spans: serde_json::Value,
    pub session_class: String,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct BackingItemRow {
    pub work_item_id: i64,
    pub session_id: i64,
    pub kind: String,
    pub task_id: String,
    pub owner_agent_id: Option<String>,
    pub description: Option<String>,
    pub launched_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl crate::store::persistence::dossier_repo::DossierDb<'_> {
    pub async fn load_directive_sources(
        self,
        session_ids: &[i64],
    ) -> Result<Vec<DirectiveSourceRow>, PersistError> {
        crate::metrics::time_db("load_directive_sources", async move {
            let rows = sqlx::query_file_as!(
                DirectiveSourceRow,
                "src/store/persistence/dossier_entries/directive_sources.sql",
                session_ids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn load_promise_sources(
        self,
        session_ids: &[i64],
    ) -> Result<Vec<PromiseSourceRow>, PersistError> {
        crate::metrics::time_db("load_promise_sources", async move {
            let rows = sqlx::query_file_as!(
                PromiseSourceRow,
                "src/store/persistence/dossier_entries/promise_sources.sql",
                session_ids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn load_backing_items(
        self,
        session_ids: &[i64],
    ) -> Result<Vec<BackingItemRow>, PersistError> {
        crate::metrics::time_db("load_backing_items", async move {
            let rows = sqlx::query_file_as!(
                BackingItemRow,
                "src/store/persistence/dossier_entries/backing_items.sql",
                session_ids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn work_registry_first_launch(self) -> Result<Option<DateTime<Utc>>, PersistError> {
        crate::metrics::time_db("work_registry_first_launch", async move {
            let first = sqlx::query_scalar!("SELECT min(launched_at) FROM agents.work_items")
                .fetch_one(&self.pool)
                .await?;
            Ok(first)
        })
        .await
    }

    pub async fn promise_close_candidates(
        self,
        session_ids: &[i64],
        settle_minutes: i64,
    ) -> Result<Vec<EntryClose>, PersistError> {
        crate::metrics::time_db("promise_close_candidates", async move {
            let rows = sqlx::query_file_as!(
                EntryClose,
                "src/store/persistence/dossier_entries/promise_close.sql",
                session_ids,
                settle_minutes,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn list_entries_as_of(
        self,
        session_id: i64,
        as_of: DateTime<Utc>,
    ) -> Result<Vec<DossierEntryRow>, PersistError> {
        crate::metrics::time_db("list_entries_as_of", async move {
            let rows = sqlx::query_file_as!(
                DossierEntryRow,
                "src/store/persistence/dossier_entries/entries_as_of.sql",
                session_id,
                as_of,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
