use uuid::Uuid;

use super::ObservationLimits;
use super::tests_observation_support::{fixture, observation};
use crate::runtime::{ModelCallObservation, UsageSnapshot};

#[tokio::test]
async fn observation_identity_refusal_rolls_back_new_session() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let incoming = Uuid::new_v4();
    let limits = ObservationLimits {
        max_bytes: 1024,
        ..Default::default()
    };
    assert!(
        f.handle
            .ensure_runtime_session_with_limits(incoming, &limits)
            .await
            .is_err()
    );
    assert!(
        sqlx::query_scalar!(
            "SELECT id FROM sessions.sessions WHERE session_uuid = $1",
            incoming
        )
        .fetch_optional(f.handle.pool())
        .await?
        .is_none()
    );
    let id = f.handle.ensure_runtime_session(incoming).await?;
    assert_eq!(f.handle.ensure_runtime_session(incoming).await?, id);
    let charged = sqlx::query_scalar!(
        "SELECT retained_bytes FROM runtime.session_identities WHERE session_id = $1",
        id
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(charged, 65_536);
    f.cleanup().await
}

#[tokio::test]
async fn observation_usage_prefers_wire_without_summing_overlapping_otel_or_unknown_scope()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let mut o = observation(f.session)?;
    o.source.kind = "otel".into();
    o.model_call = Some(ModelCallObservation {
        call_id: "otel-one".into(),
        status: Some("completed".into()),
        duration_ms: None,
        http_status: None,
        usage: Some(UsageSnapshot {
            scope: "own".into(),
            basis: "call".into(),
            input_tokens: Some(100),
            output_tokens: Some(20),
            ..Default::default()
        }),
    });
    f.handle.apply_session_observation(&o).await?;
    assert_eq!(
        f.handle.session_observations(&[f.session]).await?[0].input_tokens,
        Some(100)
    );
    o.observation_id = Uuid::new_v4();
    o.source.kind = "responses_wire".into();
    let call = o
        .model_call
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("call missing"))?;
    call.call_id = "wire-one".into();
    call.usage
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("usage missing"))?
        .input_tokens = Some(120);
    f.handle.apply_session_observation(&o).await?;
    let row = &f.handle.session_observations(&[f.session]).await?[0];
    assert_eq!(row.input_tokens, Some(120));
    assert_eq!(row.model_call_count, 1);
    o.observation_id = Uuid::new_v4();
    let call = o
        .model_call
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("call missing"))?;
    call.call_id = "unknown-scope".into();
    call.usage
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("usage missing"))?
        .scope = "unknown".into();
    f.handle.apply_session_observation(&o).await?;
    assert_eq!(
        f.handle.session_observations(&[f.session]).await?[0].input_tokens,
        Some(120)
    );
    o.observation_id = Uuid::new_v4();
    o.usage = Some(UsageSnapshot {
        scope: "thread".into(),
        basis: "provider_cumulative".into(),
        input_tokens: Some(500),
        ..Default::default()
    });
    f.handle.apply_session_observation(&o).await?;
    assert_eq!(
        f.handle.session_observations(&[f.session]).await?[0].input_tokens,
        Some(500)
    );
    f.cleanup().await
}

#[tokio::test]
async fn observation_outbox_rotates_unavailable_backlog_and_settles_only_mailbox_receipts()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    sqlx::query!(
        "WITH originals AS (
        INSERT INTO mailbox.messages (message_uuid, to_session_id, body)
        SELECT gen_random_uuid(), $1, 'backlog fixture' FROM generate_series(1, 105)
        RETURNING message_uuid, body)
        INSERT INTO mailbox.outbox (message_uuid, to_session, runtime, body, generation)
        SELECT message_uuid, $2, 'codex', body, 0 FROM originals",
        f.session_db,
        f.session
    )
    .execute(f.handle.pool())
    .await?;
    let first = sqlx::query_file_scalar!("src/mail_outbox/open_messages.sql", 100_i64)
        .fetch_all(f.handle.pool())
        .await?;
    let second = sqlx::query_file_scalar!("src/mail_outbox/open_messages.sql", 100_i64)
        .fetch_all(f.handle.pool())
        .await?;
    assert_eq!(first.len(), 100);
    assert_eq!(second.iter().filter(|id| !first.contains(id)).count(), 5);
    let queued = first[0];
    assert!(
        f.handle
            .reserve_runtime_mail_notification(f.session, queued)
            .await?
    );
    f.handle
        .finish_runtime_mail_notification(queued, "queued", None)
        .await?;
    assert_eq!(f.handle.reconcile_runtime_mail_outbox().await?, 0);
    let available = sqlx::query_file_scalar!("src/mail_outbox/open_messages.sql", 200_i64)
        .fetch_all(f.handle.pool())
        .await?;
    assert!(!available.contains(&queued));
    sqlx::query!(
        "UPDATE mailbox.messages SET consumed_at = now() WHERE message_uuid = $1",
        queued
    )
    .execute(f.handle.pool())
    .await?;
    assert_eq!(f.handle.reconcile_runtime_mail_outbox().await?, 1);
    let status = sqlx::query_scalar!(
        "SELECT status FROM mailbox.outbox WHERE message_uuid = $1",
        queued
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(status, "delivered");
    let consumed = sqlx::query_scalar!(
        "SELECT stage FROM mailbox.outbox_stages WHERE message_uuid = $1 AND stage = 'consumed'",
        queued
    )
    .fetch_one(f.handle.pool())
    .await?;
    assert_eq!(consumed, "consumed");
    f.cleanup().await
}
