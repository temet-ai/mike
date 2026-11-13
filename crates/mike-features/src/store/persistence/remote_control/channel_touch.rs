use chrono::{DateTime, Utc};

use super::{CallInsertParams, RemoteControlCall};
use crate::store::persistence::PersistError;

const LAST_SEEN_DEBOUNCE_SECS: i32 = 10;
const TOUCH_ATTEMPTS: usize = 2;

pub struct ChannelReading {
    pub id: i64,
    pub previous_status: Option<String>,
    pub previous_status_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChannelTouch<'a> {
    pub cse_id: &'a str,
    pub client_session_id: Option<&'a str>,
    pub worker_epoch: Option<i64>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub heartbeat_at: Option<DateTime<Utc>>,
    pub idle_seconds: Option<i32>,
    pub interval_seconds: Option<i32>,
    pub worker_status: Option<&'a str>,
    pub worker_status_at: Option<DateTime<Utc>>,
    pub delivery_at: Option<DateTime<Utc>>,
    pub stream_opened_at: Option<DateTime<Utc>>,
    pub local_session_uuid: Option<uuid::Uuid>,
    pub local_cwd: Option<&'a str>,
    pub client_version: Option<&'a str>,
    pub post_turn_summary: Option<&'a serde_json::Value>,
    pub post_turn_summary_at: Option<DateTime<Utc>>,
}

pub fn supersedes(candidate: Option<DateTime<Utc>>, current: Option<DateTime<Utc>>) -> bool {
    match (candidate, current) {
        (Some(candidate), Some(current)) => candidate >= current,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

fn later(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    a.max(b)
}

impl<'a> ChannelTouch<'a> {
    pub fn merge(calls: &[RemoteControlCall<'a>], params: &[CallInsertParams]) -> Option<Self> {
        let mut pairs = calls.iter().zip(params);
        let (first, first_params) = pairs.next()?;
        let mut touch = Self::of(first, first_params);
        for (call, call_params) in pairs {
            touch.absorb(call, call_params);
        }
        Some(touch)
    }

    fn of(call: &RemoteControlCall<'a>, p: &CallInsertParams) -> Self {
        Self {
            cse_id: call.cse_id,
            client_session_id: call.signals.client_session_id,
            worker_epoch: call.signals.worker_epoch,
            first_seen_at: call.created_at,
            last_seen_at: call.created_at,
            heartbeat_at: p.times.heartbeat_at,
            idle_seconds: call.signals.idle_seconds,
            interval_seconds: call.signals.current_interval_seconds,
            worker_status: call.signals.worker_status,
            worker_status_at: p.times.worker_status_at,
            delivery_at: p.times.delivery_at,
            stream_opened_at: p.times.stream_opened_at,
            local_session_uuid: p.local_session_uuid,
            local_cwd: call.signals.local_cwd,
            client_version: call.signals.client_version,
            post_turn_summary: call.signals.post_turn_summary,
            post_turn_summary_at: p.times.post_turn_summary_at,
        }
    }

    fn absorb(&mut self, call: &RemoteControlCall<'a>, p: &CallInsertParams) {
        let signals = &call.signals;
        self.client_session_id = signals.client_session_id.or(self.client_session_id);
        self.worker_epoch = signals.worker_epoch.or(self.worker_epoch);
        self.first_seen_at = self.first_seen_at.min(call.created_at);
        self.last_seen_at = self.last_seen_at.max(call.created_at);
        if supersedes(p.times.heartbeat_at, self.heartbeat_at) {
            self.heartbeat_at = p.times.heartbeat_at;
            self.idle_seconds = signals.idle_seconds;
            self.interval_seconds = signals.current_interval_seconds;
        }
        if supersedes(p.times.worker_status_at, self.worker_status_at) {
            self.worker_status = signals.worker_status;
            self.worker_status_at = p.times.worker_status_at;
        }
        self.delivery_at = later(self.delivery_at, p.times.delivery_at);
        self.stream_opened_at = later(self.stream_opened_at, p.times.stream_opened_at);
        if self.local_session_uuid.is_none() {
            self.local_session_uuid = p.local_session_uuid;
            self.local_cwd = signals.local_cwd;
            self.client_version = signals.client_version;
        }
        if supersedes(p.times.post_turn_summary_at, self.post_turn_summary_at) {
            self.post_turn_summary = signals.post_turn_summary;
            self.post_turn_summary_at = p.times.post_turn_summary_at;
        }
    }

    pub async fn apply(
        &self,
        conn: &mut sqlx::PgConnection,
    ) -> Result<ChannelReading, PersistError> {
        for _ in 0..TOUCH_ATTEMPTS {
            if let Some(reading) = self.apply_once(&mut *conn).await? {
                return Ok(reading);
            }
        }
        Err(PersistError::DataInvariant(format!(
            "remote_control channel {} could not be resolved after {TOUCH_ATTEMPTS} attempts",
            self.cse_id
        )))
    }

    async fn apply_once(
        &self,
        conn: &mut sqlx::PgConnection,
    ) -> Result<Option<ChannelReading>, PersistError> {
        crate::metrics::time_db("touch_remote_control_channel", async move {
            let row = sqlx::query_file!(
                "src/store/persistence/remote_control/touch_channel.sql",
                self.cse_id,
                self.client_session_id,
                self.worker_epoch,
                self.last_seen_at,
                self.heartbeat_at,
                self.idle_seconds,
                self.interval_seconds,
                self.worker_status,
                self.worker_status_at,
                self.delivery_at,
                self.stream_opened_at,
                self.local_session_uuid,
                self.local_cwd,
                self.client_version,
                self.post_turn_summary,
                self.post_turn_summary_at,
                LAST_SEEN_DEBOUNCE_SECS,
                self.first_seen_at,
            )
            .fetch_one(conn)
            .await?;
            Ok::<_, PersistError>(row.id.map(|id| ChannelReading {
                id,
                previous_status: row.previous_status,
                previous_status_at: row.previous_status_at,
            }))
        })
        .await
    }
}
