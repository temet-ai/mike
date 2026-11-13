use chrono::{DateTime, Utc};

use super::PersistError;

pub const PATH_HEARTBEAT: &str = "/worker/heartbeat";
pub const PATH_DELIVERY: &str = "/worker/events/delivery";
pub const PATH_STREAM: &str = "/worker/events/stream";
const RENEW_WHEN_UNDER_MINUTES: i32 = 10;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RemoteControlSignals<'a> {
    pub client_session_id: Option<&'a str>,
    pub worker_epoch: Option<i64>,
    pub worker_status: Option<&'a str>,
    pub idle_seconds: Option<i32>,
    pub current_interval_seconds: Option<i32>,

    pub is_compaction: Option<bool>,
    pub session_agent_id: Option<&'a str>,
    pub preserved_event_ids: Option<&'a [&'a str]>,

    pub post_turn_summary: Option<&'a serde_json::Value>,
    pub local_session_uuid: Option<&'a str>,
    pub local_cwd: Option<&'a str>,
    pub client_version: Option<&'a str>,
}

#[derive(Debug, Clone, Copy)]
pub struct RemoteControlPayload<'a> {
    pub ordinal: i32,
    pub payload_type: &'a str,
    pub body: Option<&'a [u8]>,
    pub body_sha256: [u8; 32],
    pub body_size: i32,
}

#[derive(Debug, Clone)]
pub struct RemoteControlCall<'a> {
    pub cse_id: &'a str,
    pub method: &'a str,
    pub path: &'a str,
    pub http_status: Option<i32>,
    pub request_body: Option<&'a [u8]>,
    pub request_body_sha256: Option<[u8; 32]>,
    pub request_body_size: Option<i32>,
    pub response_body: Option<&'a [u8]>,
    pub response_body_sha256: Option<[u8; 32]>,
    pub response_body_size: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub signals: RemoteControlSignals<'a>,
    pub payloads: &'a [RemoteControlPayload<'a>],
}

struct PayloadTally {
    count: Option<i32>,
    kept_count: Option<i32>,
    type_counts: Option<serde_json::Value>,
}

impl PayloadTally {
    fn of(payloads: &[RemoteControlPayload<'_>]) -> Self {
        if payloads.is_empty() {
            return Self {
                count: None,
                kept_count: None,
                type_counts: None,
            };
        }
        let mut by_type: std::collections::BTreeMap<&str, i64> = std::collections::BTreeMap::new();
        let mut kept = 0i32;
        for p in payloads {
            *by_type.entry(p.payload_type).or_insert(0) += 1;
            if p.body.is_some() {
                kept += 1;
            }
        }
        Self {
            count: i32::try_from(payloads.len()).ok(),
            kept_count: Some(kept),
            type_counts: Some(serde_json::json!(by_type)),
        }
    }
}

struct RollupTimes {
    heartbeat_at: Option<DateTime<Utc>>,
    worker_status_at: Option<DateTime<Utc>>,
    delivery_at: Option<DateTime<Utc>>,
    stream_opened_at: Option<DateTime<Utc>>,
    post_turn_summary_at: Option<DateTime<Utc>>,
}

impl RollupTimes {
    fn of(call: &RemoteControlCall<'_>) -> Self {
        Self {
            heartbeat_at: (call.path == PATH_HEARTBEAT).then_some(call.created_at),
            worker_status_at: call.signals.worker_status.map(|_| call.created_at),
            delivery_at: (call.path == PATH_DELIVERY).then_some(call.created_at),
            stream_opened_at: (call.path == PATH_STREAM).then_some(call.created_at),
            post_turn_summary_at: call.signals.post_turn_summary.map(|_| call.created_at),
        }
    }
}

impl crate::store::persistence::proxy_repo::ProxyDb<'_> {
    pub async fn record_remote_control_call(
        self,
        call: RemoteControlCall<'_>,
    ) -> Result<i64, PersistError> {
        let cse_id = call.cse_id;
        self.record_remote_control_calls(std::slice::from_ref(&call))
            .await?
            .first()
            .copied()
            .ok_or_else(|| {
                PersistError::DataInvariant(format!(
                    "remote_control channel {cse_id} recorded no row for its call"
                ))
            })
    }

    async fn renew_lease_on_heartbeat(self, cse_id: &str) {
        let renewed = crate::metrics::time_db("remote_control_renew_lease_on_heartbeat", async {
            sqlx::query_file!(
                "src/store/persistence/remote_control/renew_lease_on_heartbeat.sql",
                cse_id,
                i32::try_from(super::workspace::HEARTBEAT_LEASE_MINUTES).unwrap_or(i32::MAX),
                RENEW_WHEN_UNDER_MINUTES,
            )
            .execute(&self.pool)
            .await
        })
        .await;
        if let Err(error) = renewed {
            tracing::warn!(%error, cse_id, "a heartbeat could not renew its session's roster lease");
        }
    }

    pub async fn close_remote_control_stream(
        self,
        call_id: i64,
        closed_at: DateTime<Utc>,
    ) -> Result<(), PersistError> {
        let closed = crate::metrics::time_db("close_remote_control_stream", async move {
            let closed = sqlx::query_scalar!(
                r#"
                WITH c AS (
                    UPDATE remote_control.calls
                       SET completed_at = $2
                     WHERE id = $1
                    RETURNING channel_id, created_at
                )
                UPDATE remote_control.channels ch
                   SET last_seen_at     = GREATEST(ch.last_seen_at, $2),
                       stream_closed_at = CASE
                           WHEN ch.stream_opened_at <= c.created_at
                           THEN GREATEST(ch.stream_closed_at, $2)
                           ELSE ch.stream_closed_at
                       END
                  FROM c
                 WHERE ch.id = c.channel_id
                RETURNING ch.id AS "id!"
                "#,
                call_id,
                closed_at,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok::<_, PersistError>(closed)
        })
        .await?;

        if closed.is_none() {
            return Err(PersistError::DataInvariant(format!(
                "remote_control.calls id {call_id} does not exist, so its stream close was not recorded"
            )));
        }
        Ok(())
    }
}

pub struct CallInsertParams {
    times: RollupTimes,
    tally: PayloadTally,
    request_sha: Option<[u8; 32]>,
    response_sha: Option<[u8; 32]>,
    preserved: Option<Vec<String>>,
    local_session_uuid: Option<uuid::Uuid>,
    p_ordinal: Vec<i32>,
    p_type: Vec<String>,
    p_body: Vec<Vec<u8>>,
    p_sha: Vec<Vec<u8>>,
    p_size: Vec<i32>,
}

impl CallInsertParams {
    fn build(call: &RemoteControlCall<'_>) -> Self {
        let times = RollupTimes::of(call);
        let tally = PayloadTally::of(call.payloads);
        let request_sha = call.request_body_sha256;
        let response_sha = call.response_body_sha256;
        let preserved: Option<Vec<String>> = call
            .signals
            .preserved_event_ids
            .map(|ids| ids.iter().map(|s| (*s).to_owned()).collect());

        let kept: Vec<&RemoteControlPayload<'_>> =
            call.payloads.iter().filter(|p| p.body.is_some()).collect();
        let p_ordinal: Vec<i32> = kept.iter().map(|p| p.ordinal).collect();
        let p_type: Vec<String> = kept.iter().map(|p| p.payload_type.to_owned()).collect();
        let p_body: Vec<Vec<u8>> = kept
            .iter()
            .map(|p| p.body.unwrap_or_default().to_vec())
            .collect();
        let p_sha: Vec<Vec<u8>> = kept.iter().map(|p| p.body_sha256.to_vec()).collect();
        let p_size: Vec<i32> = kept.iter().map(|p| p.body_size).collect();

        let local_session_uuid = call
            .signals
            .local_session_uuid
            .and_then(|s| uuid::Uuid::parse_str(s).ok());

        Self {
            times,
            tally,
            request_sha,
            response_sha,
            preserved,
            local_session_uuid,
            p_ordinal,
            p_type,
            p_body,
            p_sha,
            p_size,
        }
    }
}

pub mod call_batch;
pub mod channel_link;
pub mod channel_touch;
pub mod injections;
pub mod link_health;
pub mod link_sweep;
pub mod reads;

pub use reads::ChannelSnapshotRow;

pub use link_health::ChannelLinkCounts;
pub use link_sweep::ChannelLinkSweepCandidate;

pub use injections::PushSource;

#[cfg(test)]
mod tests;
