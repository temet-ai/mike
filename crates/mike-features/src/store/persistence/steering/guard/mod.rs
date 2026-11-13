pub mod labels;
pub mod metrics;
pub mod replay;

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::PersistError;

pub use labels::GuardLabelCandidate;
pub use metrics::{GuardMetricRow, GuardMetricsWindow};
pub use replay::{GuardReplayTurn, ReplayWindow};

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct GuardSteererRow {
    pub id: i64,
    pub enabled: bool,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct QuestionPolicyRow {
    pub question: String,
    pub mode: String,
    pub updated_at: DateTime<Utc>,
    pub updated_via: String,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct ChannelReachRow {
    pub stream_opened_at: Option<DateTime<Utc>>,
    pub stream_closed_at: Option<DateTime<Utc>>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub last_heartbeat_interval_seconds: Option<i32>,
}

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub async fn guard_steerer(self) -> Result<Option<GuardSteererRow>, PersistError> {
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

    pub async fn steerer_config(
        self,
        steerer_id: i64,
    ) -> Result<Option<serde_json::Value>, PersistError> {
        crate::metrics::time_db("steerer_config", async move {
            Ok(sqlx::query_scalar!(
                "SELECT config FROM steering.steerers WHERE id = $1",
                steerer_id,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn question_policies(
        self,
        steerer_id: i64,
    ) -> Result<Vec<QuestionPolicyRow>, PersistError> {
        crate::metrics::time_db("question_policies", async move {
            Ok(sqlx::query_as!(
                QuestionPolicyRow,
                "SELECT question, mode, updated_at, updated_via
                   FROM steering.question_policies WHERE steerer_id = $1 ORDER BY question",
                steerer_id,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn set_question_mode(
        self,
        steerer_id: i64,
        question: &str,
        mode: &str,
        via: &str,
    ) -> Result<QuestionPolicyRow, PersistError> {
        crate::metrics::time_db("set_question_mode", async move {
            Ok(sqlx::query_as!(
                QuestionPolicyRow,
                r#"INSERT INTO steering.question_policies (steerer_id, question, mode, updated_via)
                   VALUES ($1, $2, $3, $4)
                   ON CONFLICT (steerer_id, question) DO UPDATE
                      SET mode = EXCLUDED.mode, updated_at = now(),
                          updated_via = EXCLUDED.updated_via
                   RETURNING question AS "question!", mode AS "mode!", updated_at AS "updated_at!",
                             updated_via AS "updated_via!""#,
                steerer_id,
                question,
                mode,
                via,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn steerer_calls_today(
        self,
        steerer_id: i64,
        role: &str,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("steerer_calls_today", async move {
            Ok(sqlx::query_scalar!(
                r#"SELECT count(*) AS "count!" FROM llm.calls
                    WHERE role = $2 AND steerer_id = $1 AND created_at >= date_trunc('day', now())"#,
                steerer_id,
                role,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn last_tool_of_turn(
        self,
        operator_turn_id: i64,
    ) -> Result<Option<String>, PersistError> {
        crate::metrics::time_db("last_tool_of_turn", async move {
            Ok(sqlx::query_scalar!(
                "SELECT u.tool_name
                   FROM requests.requests q
                   JOIN tools.uses u ON u.request_id = q.id
                  WHERE q.operator_turn_id = $1
                  ORDER BY u.created_at DESC, u.request_id DESC, u.ord DESC
                  LIMIT 1",
                operator_turn_id,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn channel_reach_rows(
        self,
        session_db_id: i64,
        session_uuid: Option<uuid::Uuid>,
    ) -> Result<Vec<ChannelReachRow>, PersistError> {
        crate::metrics::time_db("channel_reach_rows", async move {
            Ok(match session_uuid {
                Some(session_uuid) => {
                    sqlx::query_as!(
                        ChannelReachRow,
                        "SELECT stream_opened_at, stream_closed_at, last_seen_at,
                                last_heartbeat_interval_seconds
                           FROM remote_control.channels
                          WHERE session_id = $1 OR local_session_uuid = $2",
                        session_db_id,
                        session_uuid,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
                None => {
                    sqlx::query_as!(
                        ChannelReachRow,
                        "SELECT stream_opened_at, stream_closed_at, last_seen_at,
                                last_heartbeat_interval_seconds
                           FROM remote_control.channels
                          WHERE session_id = $1",
                        session_db_id,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
            })
        })
        .await
    }

    pub async fn verifier_pattern(self) -> Result<Option<String>, PersistError> {
        crate::metrics::time_db("verifier_pattern", async move {
            let row = sqlx::query_scalar!(
                "SELECT pattern FROM steering.steerers
                  WHERE name = 'verifier' AND deleted_at IS NULL ORDER BY id LIMIT 1",
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.flatten())
        })
        .await
    }

    pub async fn session_project_id(self, session_db_id: i64) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db("session_project_id", async move {
            Ok(sqlx::query_scalar!(
                "SELECT project_id FROM sessions.sessions WHERE id = $1",
                session_db_id,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }
}
