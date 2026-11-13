use crate::store::persistence::launch_repo::LaunchPersist as _;
use chrono::{DateTime, Utc};

use super::{
    BACKOFF_BASE_SECONDS, BACKOFF_CAP_SECONDS, DELIVER_RECENT_HOURS, MAX_DELIVERY_ATTEMPTS,
    SETTLED_SECONDS, TAIL_BYTES,
};
use crate::store::persistence::work_item_attention::AttentionRecord;
use crate::store::persistence::{PersistError, PersistHandle};
use crate::work_items::attention::Attention;
use crate::work_items::output_end::{Ended, ended};

pub const STALE_REASON: &str = "older than the delivery window when delivery was on";
pub const RESOLVED_REASON: &str = "no longer true on the rescan before delivery";

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Nothing,
    SettleStale,
    Mail,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Retry {
    Due,
    Waiting,
    Exhausted,
}

pub fn may_act(recorded: &AttentionRecord) -> bool {
    !recorded.delivered && recorded.delivery_attempts < MAX_DELIVERY_ATTEMPTS
}

pub fn decide(
    deliver: bool,
    item: &Attention,
    recorded: &AttentionRecord,
    now: DateTime<Utc>,
) -> Decision {
    if deliver && !recorded.delivered && !is_recent(item, now) {
        if recorded.disposition == "pending" {
            return Decision::SettleStale;
        }
        return Decision::Nothing;
    }
    if wants_mail(deliver, recorded, item, now) {
        Decision::Mail
    } else {
        Decision::Nothing
    }
}

pub fn wants_mail(
    deliver: bool,
    recorded: &AttentionRecord,
    item: &Attention,
    now: DateTime<Utc>,
) -> bool {
    deliver
        && !recorded.delivered
        && is_recent(item, now)
        && matches!(
            retry_state(
                recorded.delivery_attempts,
                recorded.last_delivery_attempt_at,
                now
            ),
            Retry::Due
        )
}

pub fn delivery_backoff(attempts: i32) -> chrono::Duration {
    let doublings = u32::try_from(attempts.saturating_sub(1))
        .unwrap_or(0)
        .min(20);
    let seconds = BACKOFF_BASE_SECONDS
        .saturating_mul(1i64 << doublings)
        .min(BACKOFF_CAP_SECONDS);
    chrono::Duration::seconds(seconds)
}

pub fn retry_state(
    attempts: i32,
    last_attempt: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Retry {
    if attempts >= MAX_DELIVERY_ATTEMPTS {
        return Retry::Exhausted;
    }
    match last_attempt {
        Some(at) if attempts > 0 && now < at + delivery_backoff(attempts) => Retry::Waiting,
        _ => Retry::Due,
    }
}

pub fn next_retry<'a>(records: impl Iterator<Item = &'a AttentionRecord>) -> Option<DateTime<Utc>> {
    records
        .filter(|r| may_act(r) && r.delivery_attempts > 0)
        .filter_map(|r| {
            r.last_delivery_attempt_at
                .map(|at| at + delivery_backoff(r.delivery_attempts))
        })
        .min()
}

pub fn attention_message_uuid(attention_id: i64) -> uuid::Uuid {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(b"wkattn01");
    bytes[8..].copy_from_slice(&attention_id.to_be_bytes());
    uuid::Builder::from_custom_bytes(bytes).into_uuid()
}

fn is_recent(item: &Attention, now: DateTime<Utc>) -> bool {
    item.since > now - chrono::Duration::hours(DELIVER_RECENT_HOURS)
}

pub fn current<'a>(still_true: Option<&'a [Attention]>, item: &Attention) -> Option<&'a Attention> {
    still_true?
        .iter()
        .find(|other| other.kind == item.kind && other.subject == item.subject)
}

pub async fn settle(
    persist: &PersistHandle,
    recorded: &AttentionRecord,
    disposition: &str,
    reason: &str,
) -> Result<(), PersistError> {
    persist
        .launch()
        .settle_work_item_attention(recorded.id, disposition, reason)
        .await
}

pub async fn rescan(persist: &PersistHandle, session_db_id: i64) -> Result<(), PersistError> {
    persist.launch().backfill_work_items(session_db_id).await?;
    for job in persist.launch().open_background_jobs(session_db_id).await? {
        let Some((ended, at)) = read_end(&job.output_path).await else {
            continue;
        };
        if persist
            .launch()
            .close_job_from_its_output(job.id, ended, at)
            .await?
            > 0
        {
            tracing::info!(
                session_db_id,
                output = %job.output_path,
                state = ended.state,
                "work_registry: job closed from the last line of its output"
            );
        }
    }
    Ok(())
}

async fn read_end(path: &str) -> Option<(Ended, DateTime<Utc>)> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let mut file = tokio::fs::File::open(path).await.ok()?;
    let meta = file.metadata().await.ok()?;
    let written: DateTime<Utc> = meta.modified().ok()?.into();
    if Utc::now() - written < chrono::Duration::seconds(SETTLED_SECONDS) {
        return None;
    }
    let from = meta.len().saturating_sub(TAIL_BYTES);
    file.seek(std::io::SeekFrom::Start(from)).await.ok()?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).await.ok()?;
    ended(&String::from_utf8_lossy(&tail)).map(|end| (end, written))
}

pub async fn mail(
    persist: &PersistHandle,
    session_uuid: &str,
    item: &Attention,
    recorded: &AttentionRecord,
) -> AttentionRecord {
    let body = format!("[mike work registry] {}", item.message);
    let outcome = persist
        .launch()
        .deliver_work_item_attention(recorded.id, attention_message_uuid(recorded.id), &body)
        .await;
    let failure = match outcome {
        Ok(true) => return delivered(recorded),
        Ok(false) => "the session has no mailbox row to deliver to".to_owned(),
        Err(e) => e.to_string(),
    };
    log_failed_delivery(session_uuid, item, recorded, &failure);
    record_failed_attempt(persist, recorded.id, &failure).await;
    failed_once(recorded, Utc::now())
}

pub fn delivered(recorded: &AttentionRecord) -> AttentionRecord {
    AttentionRecord {
        delivered: true,
        disposition: "delivered".to_owned(),
        ..recorded.clone()
    }
}

pub fn failed_once(recorded: &AttentionRecord, at: DateTime<Utc>) -> AttentionRecord {
    let attempts = recorded.delivery_attempts + 1;
    AttentionRecord {
        delivery_attempts: attempts,
        last_delivery_attempt_at: Some(at),
        disposition: if attempts >= MAX_DELIVERY_ATTEMPTS {
            "given_up".to_owned()
        } else {
            recorded.disposition.clone()
        },
        ..recorded.clone()
    }
}

fn log_failed_delivery(
    session_uuid: &str,
    item: &Attention,
    recorded: &AttentionRecord,
    failure: &str,
) {
    log_attempt_failed(session_uuid, item, recorded, failure);
    if recorded.delivery_attempts + 1 >= MAX_DELIVERY_ATTEMPTS {
        log_given_up(session_uuid, item);
    }
}

fn log_attempt_failed(
    session_uuid: &str,
    item: &Attention,
    recorded: &AttentionRecord,
    failure: &str,
) {
    tracing::warn!(
        session_id = %session_uuid,
        kind = item.kind,
        subject = %item.subject,
        attempts = recorded.delivery_attempts + 1,
        error = %failure,
        "work_registry: attention delivery failed"
    );
}

fn log_given_up(session_uuid: &str, item: &Attention) {
    tracing::warn!(
        session_id = %session_uuid,
        kind = item.kind,
        subject = %item.subject,
        "work_registry: attention delivery given up after the last attempt"
    );
}

async fn record_failed_attempt(persist: &PersistHandle, id: i64, failure: &str) {
    if let Err(e) = persist
        .launch()
        .note_work_item_attention_delivery_failed(id, MAX_DELIVERY_ATTEMPTS, failure)
        .await
    {
        tracing::warn!(error = %e, "work_registry: could not record the failed attempt");
    }
}
