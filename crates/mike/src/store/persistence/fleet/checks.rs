use chrono::{DateTime, Utc};

use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeCheckRow {
    pub id: i64,
    pub run_id: String,
    pub state: String,
    pub state_after: Option<String>,
    pub command: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
    pub output: String,
    pub askers: Vec<String>,
    pub told: Vec<String>,
}

impl TreeCheckRow {
    pub fn passed(&self) -> bool {
        self.exit_code == Some(0)
    }

    pub fn moved(&self) -> bool {
        self.state_after
            .as_deref()
            .is_some_and(|after| after != self.state)
    }
}

impl PersistHandle {
    pub async fn begin_tree_check(
        &self,
        run_id: &str,
        state: &str,
        command: &str,
        asker: &str,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("launch_begin_tree_check", async move {
            let row = sqlx::query_file!(
                "src/store/persistence/fleet/begin_tree_check.sql",
                run_id,
                state,
                command,
                asker,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(row.id)
        })
        .await
    }

    pub async fn finish_tree_check(
        &self,
        id: i64,
        exit_code: Option<i32>,
        output: &str,
        state_after: Option<&str>,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("launch_finish_tree_check", async move {
            sqlx::query_file!(
                "src/store/persistence/fleet/finish_tree_check.sql",
                id,
                exit_code,
                output,
                state_after,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn abandon_expired_fleet_checks(
        &self,
        run_id: &str,
        older_than_seconds: f64,
        note: &str,
    ) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("launch_abandon_expired_fleet_checks", async move {
            Ok(sqlx::query_file_scalar!(
                "src/store/persistence/fleet/abandon_expired_checks.sql",
                run_id,
                older_than_seconds,
                note,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn abandon_unrun_fleet_check(
        &self,
        receipt: i64,
        run_id: &str,
        note: &str,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("launch_abandon_unrun_fleet_check", async move {
            let hit: Option<i64> = sqlx::query_scalar(include_str!("abandon_unrun_check.sql"))
                .bind(receipt)
                .bind(run_id)
                .bind(note)
                .fetch_optional(&self.pool)
                .await?;
            Ok(hit.is_some())
        })
        .await
    }

    pub async fn mark_tree_check_executing(&self, id: i64) -> Result<(), PersistError> {
        crate::metrics::time_db("launch_mark_tree_check_executing", async move {
            sqlx::query_file!(
                "src/store/persistence/fleet/mark_tree_check_executing.sql",
                id
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn measured_tree_check_seconds(
        &self,
        command: &str,
    ) -> Result<Option<f64>, PersistError> {
        crate::metrics::time_db("launch_measured_tree_check_seconds", async move {
            Ok(sqlx::query_file_scalar!(
                "src/store/persistence/fleet/measured_tree_check_seconds.sql",
                command.trim()
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn find_tree_check(
        &self,
        run_id: &str,
        state: &str,
    ) -> Result<Option<TreeCheckRow>, PersistError> {
        crate::metrics::time_db("launch_find_tree_check", async move {
            let row = sqlx::query_file_as!(
                TreeCheckRow,
                "src/store/persistence/fleet/find_tree_check.sql",
                run_id,
                state,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub async fn latest_tree_check(
        &self,
        run_id: &str,
    ) -> Result<Option<TreeCheckRow>, PersistError> {
        crate::metrics::time_db("launch_latest_tree_check", async move {
            let row = sqlx::query_file_as!(
                TreeCheckRow,
                "src/store/persistence/fleet/latest_tree_check.sql",
                run_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub async fn add_tree_check_asker(&self, id: i64, seat: &str) -> Result<(), PersistError> {
        crate::metrics::time_db("launch_add_tree_check_asker", async move {
            sqlx::query_file!(
                "src/store/persistence/fleet/add_tree_check_asker.sql",
                id,
                seat,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn add_tree_check_told(&self, id: i64, seat: &str) -> Result<bool, PersistError> {
        crate::metrics::time_db("launch_add_tree_check_told", async move {
            let done = sqlx::query_file!(
                "src/store/persistence/fleet/add_tree_check_told.sql",
                id,
                seat,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected() == 1)
        })
        .await
    }
}
