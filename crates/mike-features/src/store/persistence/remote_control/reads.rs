use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::PersistError;

#[derive(Debug, Clone)]
pub struct ChannelSnapshotRow {
    pub cse_id: String,
    pub local_session_uuid: Option<Uuid>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub last_heartbeat_interval_seconds: Option<i32>,
    pub last_idle_seconds: Option<i32>,
    pub worker_status: Option<String>,
    pub worker_status_at: Option<DateTime<Utc>>,
    pub stream_open: bool,
    pub stream_opened_at: Option<DateTime<Utc>>,
    pub last_delivery_at: Option<DateTime<Utc>>,
    pub post_turn_summary: Option<serde_json::Value>,
    pub post_turn_summary_at: Option<DateTime<Utc>>,
    pub client_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorkerStatusTransitionRow {
    pub channel_id: i64,
    pub from_status: Option<String>,
    pub to_status: String,
    pub observed_at: DateTime<Utc>,
    pub call_id: Option<i64>,
}

impl crate::store::persistence::proxy_repo::ProxyDb<'_> {
    pub async fn fetch_channel_snapshots(
        self,
        session_uuids: &[Uuid],
    ) -> Result<Vec<ChannelSnapshotRow>, PersistError> {
        crate::metrics::time_db("remote_control_fetch_channel_snapshots", async move {
            let rows = sqlx::query!(
                r#"
                SELECT
                    c.cse_id                          AS "cse_id!",
                    c.local_session_uuid              AS "local_session_uuid?",
                    c.last_heartbeat_at                AS "last_heartbeat_at?",
                    c.last_heartbeat_interval_seconds  AS "last_heartbeat_interval_seconds?",
                    c.last_idle_seconds                AS "last_idle_seconds?",
                    c.worker_status                    AS "worker_status?",
                    c.worker_status_at                 AS "worker_status_at?",
                    (c.stream_opened_at IS NOT NULL
                       AND (c.stream_closed_at IS NULL
                            OR c.stream_closed_at < c.stream_opened_at)) AS "stream_open!",
                    c.stream_opened_at                 AS "stream_opened_at?",
                    c.last_delivery_at                 AS "last_delivery_at?",
                    c.post_turn_summary                AS "post_turn_summary?",
                    c.post_turn_summary_at             AS "post_turn_summary_at?",
                    c.client_version                   AS "client_version?"
                  FROM remote_control.channels c
                 WHERE c.local_session_uuid = ANY($1::uuid[])
                "#,
                session_uuids,
            )
            .fetch_all(&self.pool)
            .await?;

            Ok(rows
                .into_iter()
                .map(|r| ChannelSnapshotRow {
                    cse_id: r.cse_id,
                    local_session_uuid: r.local_session_uuid,
                    last_heartbeat_at: r.last_heartbeat_at,
                    last_heartbeat_interval_seconds: r.last_heartbeat_interval_seconds,
                    last_idle_seconds: r.last_idle_seconds,
                    worker_status: r.worker_status,
                    worker_status_at: r.worker_status_at,
                    stream_open: r.stream_open,
                    stream_opened_at: r.stream_opened_at,
                    last_delivery_at: r.last_delivery_at,
                    post_turn_summary: r.post_turn_summary,
                    post_turn_summary_at: r.post_turn_summary_at,
                    client_version: r.client_version,
                })
                .collect())
        })
        .await
    }

    pub async fn fetch_worker_status_history(
        self,
        session_uuid: Uuid,
        limit: i64,
    ) -> Result<Vec<WorkerStatusTransitionRow>, PersistError> {
        crate::metrics::time_db("remote_control_fetch_worker_status_history", async move {
            let rows = sqlx::query_as!(
                WorkerStatusTransitionRow,
                r#"
                SELECT
                    t.channel_id  AS "channel_id!",
                    t.from_status AS "from_status?",
                    t.to_status   AS "to_status!",
                    t.observed_at AS "observed_at!",
                    t.call_id     AS "call_id?"
                  FROM remote_control.worker_status_transitions t
                  JOIN remote_control.channels c ON c.id = t.channel_id
                 WHERE c.local_session_uuid = $1
                 ORDER BY t.observed_at DESC
                 LIMIT $2
                "#,
                session_uuid,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
