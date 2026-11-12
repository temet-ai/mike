use chrono::{Duration, Utc};
use uuid::Uuid;

use super::tests_observation_support::{fixture, observation};
use crate::runtime::{RuntimeItemObservation, SessionControl};

#[tokio::test]
async fn observation_items_update_in_place_keep_page_order_and_expire_with_receipts()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let mut o = observation(f.session)?;
    o.item = Some(RuntimeItemObservation {
        item_id: "tool-one".into(),
        turn_id: Some("turn-one".into()),
        kind: "tool".into(),
        name: Some("shell".into()),
        status: Some("started".into()),
        success: None,
        exit_code: None,
    });
    f.handle.apply_session_observation(&o).await?;
    let first_seen = o.observed_at;
    o.observation_id = Uuid::new_v4();
    o.observed_at += Duration::seconds(1);
    o.item
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("item missing"))?
        .status = Some("completed".into());
    f.handle.apply_session_observation(&o).await?;
    let rows = f.handle.list_runtime_items(f.session, 20, 0).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].first_seen_at, first_seen);
    assert_eq!(rows[0].status.as_deref(), Some("completed"));
    assert_eq!(rows[0].content_availability, "not_collected");
    let swept = f
        .handle
        .sweep_runtime_observations(Utc::now() + Duration::days(91), 100)
        .await?;
    assert_eq!(swept.receipts_removed, 2);
    assert_eq!(swept.items_removed, 1);
    assert!(
        f.handle
            .list_runtime_items(f.session, 20, 0)
            .await?
            .is_empty()
    );
    assert_eq!(
        f.handle.session_observations(&[f.session]).await?[0]
            .observed_model
            .as_deref(),
        Some("actual-model")
    );
    f.cleanup().await
}

#[tokio::test]
async fn observation_notification_reservation_survives_reopen_without_consuming_mail()
-> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let mut o = observation(f.session)?;
    o.control = Some(SessionControl::HookBoundary);
    f.handle.apply_session_observation(&o).await?;
    f.handle
        .register_runtime_notification_target(f.session, "thread-one", "/tmp/codex-home")
        .await?;
    assert_eq!(
        f.handle
            .runtime_notification_target(f.session)
            .await?
            .ok_or_else(|| anyhow::anyhow!("target missing"))?
            .thread_id,
        "thread-one"
    );
    let message = Uuid::new_v4();
    sqlx::query!("INSERT INTO mailbox.messages (message_uuid, to_session_id, body) VALUES ($1, $2, 'queued fixture')",
        message, f.session_db).execute(f.handle.pool()).await?;
    assert_eq!(
        f.handle.pending_runtime_mail(f.session, 20).await?,
        vec![message]
    );
    let (a, b) = tokio::join!(
        f.handle
            .reserve_runtime_mail_notification(f.session, message),
        f.handle
            .reserve_runtime_mail_notification(f.session, message)
    );
    assert_ne!(a?, b?);
    f.handle
        .finish_runtime_mail_notification(message, "uncertain", Some("process outcome unknown"))
        .await?;
    let reopened = crate::store::persistence::PersistHandle::connect(&f.db.database_url).await?;
    assert!(
        !reopened
            .reserve_runtime_mail_notification(f.session, message)
            .await?
    );
    assert_eq!(
        reopened
            .runtime_mail_notification_state(message)
            .await?
            .as_deref(),
        Some("uncertain")
    );
    let consumed = sqlx::query_scalar!(
        "SELECT consumed_at FROM mailbox.messages WHERE message_uuid = $1",
        message
    )
    .fetch_one(reopened.pool())
    .await?;
    assert!(consumed.is_none());
    reopened.pool().close().await;
    let swept = f
        .handle
        .sweep_runtime_observations(Utc::now() + Duration::days(8), 100)
        .await?;
    assert_eq!(swept.notification_targets_removed, 1);
    f.cleanup().await
}
