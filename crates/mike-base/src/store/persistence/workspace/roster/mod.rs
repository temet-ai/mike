use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{HEARTBEAT_LEASE_MINUTES, WorkspaceRosterRow};
use crate::store::persistence::PersistError;
use crate::store::persistence::PersistHandle;
use crate::workspace::{Liveness, RosterScope};

mod assemble;
mod live_count;
mod members;
#[cfg(test)]
mod notice_tests;
#[cfg(test)]
mod roster_test_support;
mod state;

pub use live_count::LiveCount;

fn activity_cutoff() -> DateTime<Utc> {
    Utc::now() - chrono::Duration::minutes(HEARTBEAT_LEASE_MINUTES)
}

fn uncaptured_cutoff() -> DateTime<Utc> {
    Utc::now() - chrono::Duration::minutes(assemble::UNCAPTURED_GRACE_MINUTES)
}

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

struct Assembled {
    rows: Vec<WorkspaceRosterRow>,
    candidates: Vec<Uuid>,
    clock: assemble::Clock,
}

impl PersistHandle {
    pub async fn fetch_workspace_roster(
        &self,
        project_id: i64,
        scope: RosterScope,
    ) -> Result<Vec<WorkspaceRosterRow>, PersistError> {
        Ok(self.assemble_roster(project_id, scope).await?.rows)
    }

    async fn assemble_roster(
        &self,
        project_id: i64,
        scope: RosterScope,
    ) -> Result<Assembled, PersistError> {
        let members = self
            .roster_members(project_id, members::includes_released(scope))
            .await?;
        let states = self.roster_member_states(&members).await?;
        let clock = assemble::Clock::at(Utc::now());
        let rows = members
            .iter()
            .filter_map(|member| Some((member, states.get(&member.session_id)?)))
            .filter(|(member, state)| assemble::in_scope(member, state, scope, &clock))
            .map(|(member, state)| assemble::roster_row(member, state, &clock))
            .collect();
        Ok(Assembled {
            rows,
            candidates: members.iter().map(|member| member.session_uuid).collect(),
            clock,
        })
    }

    pub async fn fetch_session_liveness(
        &self,
        session_uuid: uuid::Uuid,
    ) -> Result<Option<Liveness>, PersistError> {
        let activity_cutoff = activity_cutoff();
        let uncaptured_cutoff = uncaptured_cutoff();
        crate::metrics::time_db("workspace_fetch_session_liveness", async move {
            let row = sqlx::query!(
                r#"SELECT
                       CASE
                           WHEN r.released_at IS NOT NULL OR act.lifecycle IN ('ended', 'failed') THEN 'ended'
                           WHEN act.last_request_at IS NULL AND r.registered_at < $3 THEN 'uncaptured'
                           WHEN r.lease_expires_at > now() OR act.last_request_at > $2 THEN 'live'
                           ELSE 'stale'
                       END AS "liveness!"
                   FROM workspace.session_registrations r
                   JOIN sessions.sessions s ON s.id = r.session_id
                   LEFT JOIN LATERAL (
                       SELECT observed_at AS last_request_at, lifecycle
                         FROM runtime.session_activity(r.session_id)
                   ) act ON true
                  WHERE s.session_uuid = $1
                  ORDER BY r.registered_at DESC
                  LIMIT 1"#,
                session_uuid,
                activity_cutoff,
                uncaptured_cutoff,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|r| Liveness::from_db(&r.liveness)))
        })
        .await
    }

    pub async fn find_live_session_by_pid(
        &self,
        pid: i32,
    ) -> Result<Option<uuid::Uuid>, PersistError> {
        crate::metrics::time_db("workspace_find_live_session_by_pid", async move {
            let row = sqlx::query!(
                r#"SELECT s.session_uuid AS "session_uuid!"
                   FROM workspace.session_registrations r
                   JOIN sessions.sessions s ON s.id = r.session_id
                  WHERE r.pid = $1
                    AND r.released_at IS NULL
                    AND r.reaped_at IS NULL
                    AND r.lease_expires_at > now() - interval '24 hours'
                  ORDER BY r.registered_at DESC
                  LIMIT 1"#,
                pid,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|r| r.session_uuid))
        })
        .await
    }
}

impl PersistHandle {
    pub async fn fetch_channel_snapshots(
        &self,
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
}
