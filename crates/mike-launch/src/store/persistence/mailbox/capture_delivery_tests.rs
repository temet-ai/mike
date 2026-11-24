use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::mailbox::AckOutcome;
use crate::store::persistence::test_support::{TestDb, TestDbHandle};
use mike_runtime::store::persistence::runtime::capture::{CaptureChunkInput, CaptureExchangeInput};
use mike_runtime::store::persistence::runtime_repo::RuntimePersist as _;

struct Fixture {
    db: TestDbHandle,
    handle: PersistHandle,
    recipient: Uuid,
    other: Uuid,
    message: Uuid,
}

async fn fixture() -> anyhow::Result<Fixture> {
    let test_db = TestDb::bootstrap()
        .await?
        .ok_or_else(|| anyhow::anyhow!("capture receipt tests need the disposable database"))?;
    let db = test_db.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let recipient = Uuid::new_v4();
    let other = Uuid::new_v4();
    for session in [recipient, other] {
        handle
            .launch()
            .ensure_session_exists(&session.to_string())
            .await?;
    }
    let message = Uuid::new_v4();
    handle
        .launch()
        .insert_message(message, recipient, Some(other), "full mail body")
        .await?
        .ok_or_else(|| anyhow::anyhow!("message not inserted"))?;
    Ok(Fixture {
        db,
        handle,
        recipient,
        other,
        message,
    })
}

impl Fixture {
    async fn capture(&self, session: Uuid, mutated: bool) -> anyhow::Result<Uuid> {
        let original = self.document(b"original").await?;
        let sent = if mutated {
            self.document(b"original plus full mail body").await?
        } else {
            original
        };
        let exchange = Uuid::new_v4();
        self.handle
            .runtime()
            .upsert_capture_exchange(&CaptureExchangeInput {
                id: exchange,
                session_uuid: Some(session),
                provider_thread_id: Some(session.to_string()),
                created_at: Utc::now(),
                metadata: serde_json::json!({}),
                request_document_id: original,
                sent_document_id: sent,
                response_document_id: None,
                status: "pending".into(),
            })
            .await?;
        Ok(exchange)
    }

    async fn document(&self, bytes: &[u8]) -> anyhow::Result<i64> {
        let id = self.handle.runtime().create_capture_document(None).await?;
        self.handle
            .runtime()
            .append_capture_chunks(id, 0, &[CaptureChunkInput::New(bytes.to_vec())])
            .await?;
        self.handle
            .runtime()
            .finish_capture_document(id, true)
            .await?;
        Ok(id)
    }

    async fn record(&self, capture: Uuid) -> anyhow::Result<Vec<Uuid>> {
        Ok(self
            .handle
            .launch()
            .record_capture_delivery(self.recipient, capture, &[self.message])
            .await?)
    }

    async fn attempts(&self) -> anyhow::Result<i32> {
        Ok(sqlx::query_scalar!(
            "SELECT delivery_attempts FROM mailbox.messages WHERE message_uuid = $1",
            self.message
        )
        .fetch_one(self.handle.pool())
        .await?)
    }

    async fn cleanup(self) -> anyhow::Result<()> {
        self.handle.pool().close().await;
        self.db.cleanup().await
    }
}

#[tokio::test]
async fn db_capture_delivery_replayed_and_acknowledged_preserves_owned_evidence()
-> anyhow::Result<()> {
    let f = fixture().await?;
    assert_eq!(
        f.handle
            .launch()
            .ack_message(f.recipient, f.message)
            .await?,
        AckOutcome::NeverDelivered
    );
    assert!(f.handle.launch().claim_message(f.message).await?);
    let unchanged = f.capture(f.recipient, false).await?;
    assert!(f.record(unchanged).await?.is_empty());
    let wrong_owner = f.capture(f.other, true).await?;
    assert!(f.record(wrong_owner).await?.is_empty());
    assert!(f.record(Uuid::new_v4()).await?.is_empty());
    assert_eq!(f.attempts().await?, 0);
    let captured = f.capture(f.recipient, true).await?;
    let (first, replay) = tokio::join!(f.record(captured), f.record(captured));
    assert_eq!(first?, vec![f.message]);
    assert_eq!(replay?, vec![f.message]);
    assert_eq!(f.attempts().await?, 1);
    assert!(
        f.handle
            .launch()
            .fetch_mail_for_drain(f.recipient, 10)
            .await?
            .is_empty()
    );
    let receipts = f
        .handle
        .launch()
        .fetch_capture_delivery_evidence(f.recipient, f.message)
        .await?;
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].exchange_uuid, captured);
    assert_delivered_thread(&f).await?;
    assert!(
        f.handle
            .launch()
            .fetch_capture_delivery_evidence(f.other, f.message)
            .await?
            .is_empty()
    );
    assert_eq!(
        f.handle.launch().ack_message(f.other, f.message).await?,
        AckOutcome::NotFound
    );
    let consumed = f
        .handle
        .launch()
        .ack_message(f.recipient, f.message)
        .await?;
    assert!(matches!(consumed, AckOutcome::Consumed(_)));
    assert_eq!(
        f.handle
            .launch()
            .ack_message(f.recipient, f.message)
            .await?,
        consumed
    );
    f.cleanup().await
}

async fn assert_delivered_thread(f: &Fixture) -> anyhow::Result<()> {
    let thread = f
        .handle
        .launch()
        .create_thread(Uuid::new_v4(), "capture receipt", None, None)
        .await?;
    sqlx::query("UPDATE mailbox.messages SET thread_id = $2 WHERE message_uuid = $1")
        .bind(f.message)
        .bind(thread.id)
        .execute(f.handle.pool())
        .await?;
    let summaries = f
        .handle
        .launch()
        .fetch_thread_summaries(&crate::store::persistence::mailbox::ThreadSummaryFilter {
            thread_uuid: Some(thread.thread_uuid),
            ..Default::default()
        })
        .await?;
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].unread_count, 1);
    assert_eq!(summaries[0].worst_rank, Some(3));
    assert!(summaries[0].oldest_pending_queued_at.is_none());
    Ok(())
}

#[tokio::test]
async fn db_capture_delivery_failure_and_retry_keep_pending_and_sent_states_distinct()
-> anyhow::Result<()> {
    let f = fixture().await?;
    assert!(f.handle.launch().claim_message(f.message).await?);
    assert!(f.handle.launch().unclaim_message(f.message).await?);
    assert_eq!(
        f.handle
            .launch()
            .fetch_mail_for_drain(f.recipient, 10)
            .await?
            .len(),
        1
    );
    assert!(f.handle.launch().claim_message(f.message).await?);
    let captured = f.capture(f.recipient, true).await?;
    f.record(captured).await?;
    assert!(!f.handle.launch().unclaim_message(f.message).await?);
    let future = Utc::now() + Duration::days(2);
    assert!(
        f.handle
            .launch()
            .fetch_stale_claimed(f.recipient, future, 10)
            .await?
            .is_empty()
    );
    assert!(
        f.handle
            .launch()
            .fetch_all_stale_claimed(future, 10)
            .await?
            .is_empty()
    );
    assert_eq!(
        f.handle
            .launch()
            .dead_letter_expired_undelivered(future, future)
            .await?
            .dead_lettered,
        0
    );
    assert_eq!(f.handle.launch().earliest_undelivered().await?, None);
    let pending = f
        .handle
        .launch()
        .fetch_pending_redelivery(f.recipient, 10)
        .await?;
    assert_eq!(pending.len(), 1);
    assert!(pending[0].row.consumed_at.is_none());
    let retry = f
        .handle
        .launch()
        .bump_redelivery_attempt(f.message, 5, 1, 60)
        .await?
        .ok_or_else(|| anyhow::anyhow!("redelivery not eligible"))?;
    assert!(retry.attempted);
    let retried = f.capture(f.recipient, true).await?;
    f.record(retried).await?;
    assert_eq!(f.attempts().await?, 2);
    assert_eq!(
        f.handle
            .launch()
            .fetch_capture_delivery_evidence(f.recipient, f.message)
            .await?
            .len(),
        2
    );
    f.cleanup().await
}

#[tokio::test]
async fn db_capture_activity_undelivered_mail_stays_pending_without_inactive_notice()
-> anyhow::Result<()> {
    let f = fixture().await?;
    sqlx::query(
        "UPDATE mailbox.messages SET queued_at = now() - interval '2 days' WHERE message_uuid = $1",
    )
    .bind(f.message)
    .execute(f.handle.pool())
    .await?;
    f.capture(f.recipient, false).await?;
    let cutoff = Utc::now() - Duration::days(1);
    let active_since = Utc::now() - Duration::hours(1);
    assert_eq!(
        f.handle
            .launch()
            .dead_letter_expired_undelivered(cutoff, active_since)
            .await?
            .dead_lettered,
        0
    );
    assert_eq!(f.handle.launch().tell_inactive_senders(cutoff).await?, 0);
    let pending = f
        .handle
        .launch()
        .fetch_mail_for_drain(f.recipient, 10)
        .await?;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].message_uuid, f.message);
    assert_eq!(
        f.handle
            .launch()
            .ack_message(f.recipient, f.message)
            .await?,
        AckOutcome::NeverDelivered
    );
    f.cleanup().await
}
