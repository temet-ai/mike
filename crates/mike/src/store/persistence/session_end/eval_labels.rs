use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct SessionEndLabelItem {
    pub end_turn_id: i64,
    pub session_id: i64,
    pub label_id: i64,
    pub label: String,
    pub tier: String,
    pub closed_at: Option<DateTime<Utc>>,
    pub session_uuid: String,
}

impl PersistHandle {
    pub async fn load_session_end_label_set(
        &self,
        labels_as_of: DateTime<Utc>,
        split: &str,
        tiers: &[String],
    ) -> Result<Vec<SessionEndLabelItem>, PersistError> {
        crate::metrics::time_db("load_session_end_label_set", async move {
            Ok(sqlx::query_file_as!(
                SessionEndLabelItem,
                "src/store/persistence/session_end/label_set.sql",
                labels_as_of,
                split,
                tiers,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn load_session_end_labels_by_source(
        &self,
        labels_as_of: DateTime<Utc>,
        tiers: &[String],
        source: Option<&str>,
    ) -> Result<Vec<SessionEndLabelItem>, PersistError> {
        crate::metrics::time_db("load_session_end_labels_by_source", async move {
            let rows = match source {
                Some(source) => {
                    sqlx::query_file_as!(
                        SessionEndLabelItem,
                        "src/store/persistence/session_end/labels_by_source.sql",
                        labels_as_of,
                        tiers,
                        source,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
                None => {
                    sqlx::query_file_as!(
                        SessionEndLabelItem,
                        "src/store/persistence/session_end/labels_by_tier.sql",
                        labels_as_of,
                        tiers,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
            };
            Ok(rows)
        })
        .await
    }

    pub async fn refresh_session_end_run(
        &self,
        run_id: i64,
        status: &str,
        report: Option<&serde_json::Value>,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("refresh_session_end_run", async move {
            sqlx::query_file!(
                "src/store/persistence/session_end/refresh_run.sql",
                run_id,
                status,
                report,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn all_session_end_items(
        &self,
        run_id: i64,
    ) -> Result<Vec<super::SessionEndItemRow>, PersistError> {
        Ok(self.list_session_end_items(run_id, i64::MAX, 0).await?.0)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn source_filter_reads_the_label_source() {
        let sql = include_str!("labels_by_source.sql");
        assert!(sql.contains("l.source = $3"));
        assert!(!sql.contains("ib.source_key = $3"));
    }
}
