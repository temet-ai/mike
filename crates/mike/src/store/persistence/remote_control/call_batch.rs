use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;

use super::channel_touch::{ChannelReading, ChannelTouch, supersedes};
use super::{CallInsertParams, PATH_HEARTBEAT, RemoteControlCall};
use crate::store::persistence::{PersistError, PersistHandle};

const BATCH_ATTEMPTS: usize = 3;

#[derive(Default)]
struct CallColumns {
    method: Vec<String>,
    path: Vec<String>,
    http_status: Vec<Option<i32>>,
    request_body: Vec<Option<Vec<u8>>>,
    request_sha: Vec<Option<Vec<u8>>>,
    request_size: Vec<Option<i32>>,
    response_body: Vec<Option<Vec<u8>>>,
    response_sha: Vec<Option<Vec<u8>>>,
    response_size: Vec<Option<i32>>,
    created_at: Vec<DateTime<Utc>>,
    completed_at: Vec<Option<DateTime<Utc>>>,
    payload_count: Vec<Option<i32>>,
    kept_count: Vec<Option<i32>>,
    type_counts: Vec<Option<JsonValue>>,
    is_compaction: Vec<Option<bool>>,
    session_agent_id: Vec<Option<String>>,
    has_preserved: Vec<bool>,
}

#[derive(Default)]
struct ChildColumns {
    preserved_ord: Vec<i32>,
    preserved_id: Vec<String>,
    payload_ord: Vec<i32>,
    payload_ordinal: Vec<i32>,
    payload_type: Vec<String>,
    payload_body: Vec<Vec<u8>>,
    payload_sha: Vec<Vec<u8>>,
    payload_size: Vec<i32>,
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct Transitions {
    pub(super) ord: Vec<i32>,
    pub(super) from_status: Vec<Option<String>>,
    pub(super) to_status: Vec<String>,
    pub(super) observed_at: Vec<DateTime<Utc>>,
}

fn ord_of(index: usize) -> i32 {
    i32::try_from(index + 1).unwrap_or(i32::MAX)
}

impl CallColumns {
    fn build(calls: &[RemoteControlCall<'_>], params: &[CallInsertParams]) -> Self {
        let mut cols = Self::default();
        for (call, p) in calls.iter().zip(params) {
            cols.method.push(call.method.to_owned());
            cols.path.push(call.path.to_owned());
            cols.http_status.push(call.http_status);
            cols.request_body
                .push(call.request_body.map(<[u8]>::to_vec));
            cols.request_sha.push(p.request_sha.map(|sha| sha.to_vec()));
            cols.request_size.push(call.request_body_size);
            cols.response_body
                .push(call.response_body.map(<[u8]>::to_vec));
            cols.response_sha
                .push(p.response_sha.map(|sha| sha.to_vec()));
            cols.response_size.push(call.response_body_size);
            cols.created_at.push(call.created_at);
            cols.completed_at.push(call.completed_at);
            cols.payload_count.push(p.tally.count);
            cols.kept_count.push(p.tally.kept_count);
            cols.type_counts.push(p.tally.type_counts.clone());
            cols.is_compaction.push(call.signals.is_compaction);
            cols.session_agent_id
                .push(call.signals.session_agent_id.map(str::to_owned));
            cols.has_preserved.push(p.preserved.is_some());
        }
        cols
    }
}

impl ChildColumns {
    fn build(params: &[CallInsertParams]) -> Self {
        let mut cols = Self::default();
        for (index, p) in params.iter().enumerate() {
            let ord = ord_of(index);
            for id in p.preserved.iter().flatten() {
                cols.preserved_ord.push(ord);
                cols.preserved_id.push(id.clone());
            }
            cols.payload_ord
                .extend(std::iter::repeat_n(ord, p.p_ordinal.len()));
            cols.payload_ordinal.extend_from_slice(&p.p_ordinal);
            cols.payload_type.extend(p.p_type.iter().cloned());
            cols.payload_body.extend(p.p_body.iter().cloned());
            cols.payload_sha.extend(p.p_sha.iter().cloned());
            cols.payload_size.extend_from_slice(&p.p_size);
        }
        cols
    }
}

impl Transitions {
    pub(super) fn replay(
        channel: &ChannelReading,
        params: &[CallInsertParams],
        calls: &[RemoteControlCall<'_>],
    ) -> Self {
        let mut out = Self::default();
        let mut status = channel.previous_status.clone();
        let mut status_at = channel.previous_status_at;
        for (index, (call, p)) in calls.iter().zip(params).enumerate() {
            let (Some(next), Some(at)) = (call.signals.worker_status, p.times.worker_status_at)
            else {
                continue;
            };
            if !supersedes(Some(at), status_at) {
                continue;
            }
            status_at = Some(at);
            if status.as_deref() == Some(next) {
                continue;
            }
            out.ord.push(ord_of(index));
            out.from_status.push(status.replace(next.to_owned()));
            out.to_status.push(next.to_owned());
            out.observed_at.push(at);
        }
        out
    }
}

impl PersistHandle {
    pub async fn record_remote_control_calls(
        &self,
        calls: &[RemoteControlCall<'_>],
    ) -> Result<Vec<i64>, PersistError> {
        let params: Vec<CallInsertParams> = calls.iter().map(CallInsertParams::build).collect();
        let Some(touch) = ChannelTouch::merge(calls, &params) else {
            return Ok(Vec::new());
        };
        if let Some(other) = calls.iter().find(|call| call.cse_id != touch.cse_id) {
            return Err(PersistError::DataInvariant(format!(
                "a remote control call batch must hold one channel; it mixes {} and {}",
                touch.cse_id, other.cse_id
            )));
        }
        let columns = CallColumns::build(calls, &params);
        let children = ChildColumns::build(&params);
        let batch = Batch {
            calls,
            params: &params,
            touch: &touch,
            columns: &columns,
            children: &children,
        };
        let ids = self.write_with_retry(&batch).await?;
        if calls.iter().any(|call| call.path == PATH_HEARTBEAT) {
            self.renew_lease_on_heartbeat(touch.cse_id).await;
        }
        Ok(ids)
    }

    async fn write_with_retry(&self, batch: &Batch<'_, '_>) -> Result<Vec<i64>, PersistError> {
        let mut attempt = 1;
        loop {
            match self.write_batch(batch).await {
                Ok(ids) => return Ok(ids),
                Err(error) if attempt < BATCH_ATTEMPTS => {
                    tracing::debug!(%error, attempt, cse_id = batch.touch.cse_id, "remote control call batch failed; retrying");
                    attempt += 1;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn write_batch(&self, batch: &Batch<'_, '_>) -> Result<Vec<i64>, PersistError> {
        let mut tx = self.pool.begin().await?;
        let channel = batch.touch.apply(&mut tx).await?;
        let transitions = Transitions::replay(&channel, batch.params, batch.calls);
        let ids = insert_calls(&mut tx, channel.id, batch, &transitions).await?;
        if ids.len() != batch.calls.len() {
            return Err(PersistError::DataInvariant(format!(
                "remote control channel {} recorded {} of {} calls; an unknown method code dropped the rest",
                batch.touch.cse_id,
                ids.len(),
                batch.calls.len()
            )));
        }
        tx.commit().await?;
        Ok(ids)
    }
}

struct Batch<'c, 'a> {
    calls: &'c [RemoteControlCall<'a>],
    params: &'c [CallInsertParams],
    touch: &'c ChannelTouch<'a>,
    columns: &'c CallColumns,
    children: &'c ChildColumns,
}

async fn insert_calls(
    conn: &mut sqlx::PgConnection,
    channel_id: i64,
    batch: &Batch<'_, '_>,
    transitions: &Transitions,
) -> Result<Vec<i64>, PersistError> {
    let c = batch.columns;
    let k = batch.children;
    crate::metrics::time_db("record_remote_control_calls", async move {
        sqlx::query_file_scalar!(
            "src/store/persistence/remote_control/record_calls.sql",
            channel_id,
            &c.method,
            &c.path,
            &c.http_status as &[Option<i32>],
            &c.request_body as &[Option<Vec<u8>>],
            &c.request_sha as &[Option<Vec<u8>>],
            &c.request_size as &[Option<i32>],
            &c.response_body as &[Option<Vec<u8>>],
            &c.response_sha as &[Option<Vec<u8>>],
            &c.response_size as &[Option<i32>],
            &c.created_at,
            &c.completed_at as &[Option<DateTime<Utc>>],
            &c.payload_count as &[Option<i32>],
            &c.kept_count as &[Option<i32>],
            &c.type_counts as &[Option<JsonValue>],
            &c.is_compaction as &[Option<bool>],
            &c.session_agent_id as &[Option<String>],
            &c.has_preserved,
            &k.preserved_ord,
            &k.preserved_id,
            &transitions.ord,
            &transitions.from_status as &[Option<String>],
            &transitions.to_status,
            &transitions.observed_at,
            &k.payload_ord,
            &k.payload_ordinal,
            &k.payload_type,
            &k.payload_body,
            &k.payload_sha,
            &k.payload_size,
        )
        .fetch_all(conn)
        .await
        .map_err(PersistError::from)
    })
    .await
}
