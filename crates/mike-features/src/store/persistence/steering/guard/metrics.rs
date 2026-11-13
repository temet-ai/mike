use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::super::PersistError;

#[derive(Debug, Clone, PartialEq)]
pub struct GuardMetricsWindow {
    pub steerer_id: i64,
    pub since: DateTime<Utc>,
    pub until: DateTime<Utc>,
    pub question: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct GuardMetricRow {
    pub uuid: uuid::Uuid,
    pub session_id: i64,
    pub operator_turn_id: i64,
    pub question: Option<String>,
    pub trigger_point: Option<String>,
    pub status: String,
    pub outcome_label: Option<String>,
    pub exemption: Option<String>,
    pub arm: Option<String>,
    pub gate_open: bool,
    pub gate_reason: Option<String>,
    pub other_g2_open: bool,
    pub other_g2b_open: bool,
    pub abstention: Option<String>,
    pub recall_sample: bool,
    pub first_vote_fires: Option<bool>,
    pub stream_open: bool,
    pub channel_live: bool,
    pub next_genuine_after_secs: Option<i64>,
    pub superseded: bool,
}

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub async fn guard_metric_rows(
        self,
        w: &GuardMetricsWindow,
    ) -> Result<Vec<GuardMetricRow>, PersistError> {
        crate::metrics::time_db("guard_metric_rows", async move {
            Ok(match w.question.as_deref() {
                Some(question) => {
                    sqlx::query_file_as!(
                        GuardMetricRow,
                        "src/store/persistence/steering/guard/metric_rows_for_question.sql",
                        w.steerer_id,
                        w.since,
                        w.until,
                        question,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
                None => {
                    sqlx::query_file_as!(
                        GuardMetricRow,
                        "src/store/persistence/steering/guard/metric_rows.sql",
                        w.steerer_id,
                        w.since,
                        w.until,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
            })
        })
        .await
    }

    pub async fn guard_audit_sessions(
        self,
        since: DateTime<Utc>,
    ) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("guard_audit_sessions", async move {
            Ok(sqlx::query_file_scalar!(
                "src/store/persistence/steering/guard/audit_sessions.sql",
                since,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
