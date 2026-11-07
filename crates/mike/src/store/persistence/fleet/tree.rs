use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::launch::fleet::background_result;
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTreeChange {
    pub path: String,
    pub change: String,
    pub content_sha: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreeChangeRow {
    pub id: i64,
    pub path: String,
    pub change: String,
    pub content_sha: Option<String>,
    pub seen_at: DateTime<Utc>,
    pub seats: Option<Vec<String>>,
    pub named: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChangedPathRow {
    pub path: String,
    pub seen_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreeResultRow {
    pub use_id: i64,
    pub seat: Option<String>,
    pub session_uuid: Option<Uuid>,
    pub command: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub is_error: Option<bool>,
    pub content: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundEnd {
    pub use_id: i64,
    pub state: String,
    pub finished_at: Option<DateTime<Utc>>,
}

impl PersistHandle {
    pub async fn background_ends(
        &self,
        use_ids: &[i64],
    ) -> Result<Vec<BackgroundEnd>, PersistError> {
        if use_ids.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db("launch_background_ends", async move {
            let rows = sqlx::query_file_as!(
                BackgroundEnd,
                "src/store/persistence/fleet/background_ends.sql",
                use_ids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn insert_tree_changes(
        &self,
        run_id: &str,
        changes: &[NewTreeChange],
    ) -> Result<u64, PersistError> {
        if changes.is_empty() {
            return Ok(0);
        }
        let paths: Vec<String> = changes.iter().map(|c| c.path.clone()).collect();
        let kinds: Vec<String> = changes.iter().map(|c| c.change.clone()).collect();
        let shas: Vec<Option<String>> = changes.iter().map(|c| c.content_sha.clone()).collect();
        crate::metrics::time_db("launch_insert_tree_changes", async move {
            let done = sqlx::query_file!(
                "src/store/persistence/fleet/insert_tree_changes.sql",
                run_id,
                &paths,
                &kinds,
                &shas as &[Option<String>],
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub async fn tree_changes_since(
        &self,
        run_id: &str,
        since: DateTime<Utc>,
        path_prefix: Option<&str>,
        limit: i64,
    ) -> Result<Vec<TreeChangeRow>, PersistError> {
        crate::metrics::time_db("launch_tree_changes_since", async move {
            let rows = sqlx::query_file_as!(
                TreeChangeRow,
                "src/store/persistence/fleet/tree_changes_since.sql",
                run_id,
                since,
                path_prefix.unwrap_or_default(),
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn tree_changed_paths_since(
        &self,
        run_id: &str,
        since: DateTime<Utc>,
    ) -> Result<Vec<ChangedPathRow>, PersistError> {
        crate::metrics::time_db("launch_tree_changed_paths_since", async move {
            let rows = sqlx::query_file_as!(
                ChangedPathRow,
                "src/store/persistence/fleet/tree_changed_paths_since.sql",
                run_id,
                since,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn tree_results(
        &self,
        run_id: &str,
        command: Option<&str>,
        since: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<TreeResultRow>, PersistError> {
        let rows: Vec<TreeResultRow> = crate::metrics::time_db("launch_tree_results", async move {
            let rows = sqlx::query_file_as!(
                TreeResultRow,
                "src/store/persistence/fleet/tree_results.sql",
                run_id,
                command.unwrap_or_default(),
                since,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok::<_, PersistError>(rows)
        })
        .await?;
        let launched: Vec<i64> = rows
            .iter()
            .filter(|r| background_result::launched_in_background(r))
            .map(|r| r.use_id)
            .collect();
        let ends = self.background_ends(&launched).await?;
        Ok(background_result::settle_background(
            rows,
            &ends,
            Utc::now(),
        ))
    }
}
