use std::collections::HashMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{HEARTBEAT_LEASE_MINUTES, WorkspaceRosterRow};
use crate::store::persistence::PersistError;
use crate::store::persistence::PersistHandle;
use crate::workspace::{Liveness, RosterScope};

const MIN_HEARTBEAT_INTERVAL_SECONDS: i32 = 20;
const HEARTBEAT_DEAD_MULTIPLIER: i64 = 3;
const UNCAPTURED_GRACE_MINUTES: i64 = 10;

fn activity_cutoff() -> DateTime<Utc> {
    Utc::now() - chrono::Duration::minutes(HEARTBEAT_LEASE_MINUTES)
}

fn uncaptured_cutoff() -> DateTime<Utc> {
    Utc::now() - chrono::Duration::minutes(UNCAPTURED_GRACE_MINUTES)
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

struct RosterQueryRow {
    session_uuid: Uuid,
    cwd: String,
    worktree: Option<String>,
    registered_via: String,
    registered_at: DateTime<Utc>,
    lease_expires_at: DateTime<Utc>,
    released_at: Option<DateTime<Utc>>,
    goal: Option<String>,
    next_steps: Option<String>,
    display_name: Option<String>,
    role: Option<String>,
    instructions: Option<String>,
    liveness: String,
    last_request_at: Option<DateTime<Utc>>,
    newest_request_number: Option<i64>,
}

impl From<RosterQueryRow> for WorkspaceRosterRow {
    fn from(r: RosterQueryRow) -> Self {
        let liveness = Liveness::from_db(&r.liveness);
        let lease_lapsed = r.lease_expires_at <= Utc::now();
        Self {
            session_uuid: r.session_uuid,
            cwd: r.cwd,
            worktree: r.worktree,
            registered_via: r.registered_via,
            registered_at: r.registered_at,
            lease_expires_at: r.lease_expires_at,
            released_at: r.released_at,
            goal: r.goal,
            next_steps: r.next_steps,
            display_name: r.display_name,
            role: r.role,
            instructions: r.instructions,
            liveness,
            last_request_at: r.last_request_at,
            newest_request_number: r.newest_request_number,
            channel_present: false,
            channel_cse_id: None,
            heartbeat_age_seconds: None,
            heartbeat_interval_seconds: None,
            idle_seconds: None,
            session_state: None,
            session_state_at: None,
            reply_channel_open: false,
            reply_channel_opened_at: None,
            last_inbound_message_at: None,
            post_turn_summary: None,
            post_turn_summary_at: None,
            client_version: None,
            liveness_source: if liveness == Liveness::Live && lease_lapsed {
                "activity"
            } else {
                "lease"
            },
        }
    }
}

impl PersistHandle {
    pub async fn fetch_workspace_roster(
        &self,
        project_id: i64,
        scope: RosterScope,
    ) -> Result<Vec<WorkspaceRosterRow>, PersistError> {
        let (allow_stale, allow_ended) = scope.filter_bools();
        let activity_cutoff = activity_cutoff();
        let uncaptured_cutoff = uncaptured_cutoff();
        crate::metrics::time_db("workspace_fetch_roster", async move {
            let rows = sqlx::query_as!(
                RosterQueryRow,
                r#"SELECT
                       s.session_uuid     AS "session_uuid!",
                       r.cwd              AS "cwd!",
                       r.worktree         AS "worktree?",
                       r.registered_via   AS "registered_via!",
                       r.registered_at    AS "registered_at!",
                       r.lease_expires_at AS "lease_expires_at!",
                       r.released_at      AS "released_at?",
                       sg.goal            AS "goal?",
                       sg.next_steps      AS "next_steps?",
                       sg.display_name    AS "display_name?",
                       sg.role            AS "role?",
                       sg.instructions    AS "instructions?",
                       CASE
                           WHEN r.released_at IS NOT NULL OR act.lifecycle IN ('ended', 'failed') THEN 'ended'
                           WHEN act.last_request_at IS NULL AND r.registered_at < $5 THEN 'uncaptured'
                           WHEN r.lease_expires_at > now() OR act.last_request_at > $4 THEN 'live'
                           ELSE 'stale'
                       END                AS "liveness!",
                       act.last_request_at       AS "last_request_at?",
                       act.newest_request_number AS "newest_request_number?"
                   FROM workspace.session_registrations r
                   JOIN sessions.sessions s ON s.id = r.session_id
                   LEFT JOIN sessions.session_goals sg ON sg.session_id = r.session_id
                   LEFT JOIN runtime.session_facts f ON f.session_id = r.session_id
                   LEFT JOIN LATERAL (
                       SELECT rq.created_at, rq.request_number
                         FROM requests.requests rq
                        WHERE rq.session_id = r.session_id AND f.session_id IS NULL
                        ORDER BY rq.created_at DESC, rq.id DESC
                        LIMIT 1
                   ) latest ON true
                   CROSS JOIN LATERAL (
                       SELECT COALESCE(f.observed_at, latest.created_at) AS last_request_at,
                              latest.request_number AS newest_request_number,
                              f.lifecycle
                   ) act
                  WHERE r.project_id = $1
                    AND ($2 OR (r.released_at IS NULL AND COALESCE(act.lifecycle, '') NOT IN ('ended', 'failed')))
                    AND ($3 OR (
                            COALESCE(act.lifecycle, '') NOT IN ('ended', 'failed')
                            AND NOT (act.last_request_at IS NULL AND r.registered_at < $5)
                            AND (r.lease_expires_at > now() OR act.last_request_at > $4)
                        ))
                  ORDER BY r.registered_at DESC"#,
                project_id,
                allow_ended,
                allow_stale,
                activity_cutoff,
                uncaptured_cutoff,
            )
            .fetch_all(&self.pool)
            .await?;

            let mut roster: Vec<WorkspaceRosterRow> =
                rows.into_iter().map(WorkspaceRosterRow::from).collect();

            self.merge_channel_liveness(&mut roster).await?;
            Ok(roster)
        })
        .await
    }

    async fn merge_channel_liveness(
        &self,
        roster: &mut [WorkspaceRosterRow],
    ) -> Result<(), PersistError> {
        let session_uuids: Vec<Uuid> = roster.iter().map(|r| r.session_uuid).collect();
        let snapshots = self.fetch_channel_snapshots(&session_uuids).await?;

        let mut by_session: HashMap<Uuid, &ChannelSnapshotRow> = HashMap::new();
        for snapshot in &snapshots {
            let Some(uuid) = snapshot.local_session_uuid else {
                continue;
            };
            by_session
                .entry(uuid)
                .and_modify(|current| {
                    if snapshot.last_heartbeat_at > current.last_heartbeat_at {
                        *current = snapshot;
                    }
                })
                .or_insert(snapshot);
        }

        let now = Utc::now();
        for row in roster.iter_mut() {
            merge_channel_snapshot(row, by_session.get(&row.session_uuid).copied(), now);
        }
        Ok(())
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

fn merge_channel_snapshot(
    row: &mut WorkspaceRosterRow,
    snapshot: Option<&ChannelSnapshotRow>,
    now: DateTime<Utc>,
) {
    let Some(snapshot) = snapshot else {
        return;
    };
    row.channel_present = true;
    row.channel_cse_id = Some(snapshot.cse_id.clone());
    row.heartbeat_age_seconds = snapshot
        .last_heartbeat_at
        .map(|hb| (now - hb).num_seconds());
    row.heartbeat_interval_seconds = snapshot.last_heartbeat_interval_seconds;
    row.idle_seconds = snapshot.last_idle_seconds;
    row.session_state = snapshot.worker_status.clone();
    row.session_state_at = snapshot.worker_status_at;
    row.reply_channel_open = snapshot.stream_open;
    row.reply_channel_opened_at = snapshot
        .stream_open
        .then_some(snapshot.stream_opened_at)
        .flatten();
    row.last_inbound_message_at = snapshot.last_delivery_at;
    row.post_turn_summary = snapshot.post_turn_summary.clone();
    row.post_turn_summary_at = snapshot.post_turn_summary_at;
    row.client_version = snapshot.client_version.clone();
    row.liveness_source = "channel";

    if row.liveness == Liveness::Ended {
        return;
    }
    let threshold_seconds = i64::from(
        snapshot
            .last_heartbeat_interval_seconds
            .unwrap_or(MIN_HEARTBEAT_INTERVAL_SECONDS)
            .max(MIN_HEARTBEAT_INTERVAL_SECONDS),
    ) * HEARTBEAT_DEAD_MULTIPLIER;
    let channel_live = row
        .heartbeat_age_seconds
        .is_some_and(|age| age <= threshold_seconds);
    row.liveness = if channel_live {
        Liveness::Live
    } else {
        Liveness::Stale
    };
}
