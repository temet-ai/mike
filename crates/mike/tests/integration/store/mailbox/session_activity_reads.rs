use chrono::{DateTime, Utc};
use mike::store::persistence::launch_repo::LaunchPersist as _;

use mike::store::persistence::PersistHandle;

use super::support::{seed_request_at, seed_session_with_ids};
use crate::test_db::TestDb;

#[tokio::test]
async fn fetch_for_session_inbox_rows_carry_recipient_activity_with_eligible_lane_filter()
-> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let (to_uuid, to_id, project_id) = seed_session_with_ids(
        &pool,
        &format!("proj-fetch-inbox-activity-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let primary_at = Utc::now() - chrono::Duration::minutes(30);
    let subagent_at = Utc::now() - chrono::Duration::minutes(5);
    seed_request_at(&pool, to_id, project_id, "primary", primary_at).await?;
    seed_request_at(&pool, to_id, project_id, "subagent", subagent_at).await?;

    handle
        .launch()
        .insert_message(uuid::Uuid::new_v4(), to_uuid, None, "hello")
        .await?;

    let result = handle
        .launch()
        .fetch_for_session_with_activity(to_uuid)
        .await?;
    assert_eq!(result.inbox.len(), 1);
    let row = &result.inbox[0];
    let close = |got: Option<DateTime<Utc>>, want: DateTime<Utc>| {
        got.is_some_and(|t| (t - want).num_seconds().abs() < 2)
    };
    assert!(
        close(row.recipient_last_request_at, subagent_at),
        "recipient_last_request_at must reflect the most recent request in \
         ANY lane, got {:?}",
        row.recipient_last_request_at
    );
    assert!(
        close(row.recipient_last_eligible_turn_at, primary_at),
        "recipient_last_eligible_turn_at must reflect only the primary/primary_first \
         lane, got {:?}",
        row.recipient_last_eligible_turn_at
    );
    assert!(result.outbox.is_empty());

    Ok(())
}

#[tokio::test]
async fn fetch_for_session_outbox_rows_carry_per_recipient_activity() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let (sender_uuid, _sender_id, _sender_project) = seed_session_with_ids(
        &pool,
        &format!("proj-fetch-outbox-sender-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let (active_uuid, active_id, active_project) = seed_session_with_ids(
        &pool,
        &format!(
            "proj-fetch-outbox-active-recipient-{}",
            uuid::Uuid::new_v4()
        ),
    )
    .await?;
    let (quiet_uuid, _quiet_id, _quiet_project) = seed_session_with_ids(
        &pool,
        &format!("proj-fetch-outbox-quiet-recipient-{}", uuid::Uuid::new_v4()),
    )
    .await?;

    seed_request_at(
        &pool,
        active_id,
        active_project,
        "primary",
        Utc::now() - chrono::Duration::minutes(10),
    )
    .await?;

    let msg_to_active = uuid::Uuid::new_v4();
    handle
        .launch()
        .insert_message(msg_to_active, active_uuid, Some(sender_uuid), "to active")
        .await?;
    let msg_to_quiet = uuid::Uuid::new_v4();
    handle
        .launch()
        .insert_message(msg_to_quiet, quiet_uuid, Some(sender_uuid), "to quiet")
        .await?;

    let result = handle
        .launch()
        .fetch_for_session_with_activity(sender_uuid)
        .await?;
    assert_eq!(result.outbox.len(), 2);
    let active_row = result
        .outbox
        .iter()
        .find(|r| r.row.message_uuid == msg_to_active)
        .ok_or_else(|| anyhow::anyhow!("missing outbox row for active recipient"))?;
    assert!(active_row.recipient_last_request_at.is_some());
    assert!(active_row.recipient_last_eligible_turn_at.is_some());

    let quiet_row = result
        .outbox
        .iter()
        .find(|r| r.row.message_uuid == msg_to_quiet)
        .ok_or_else(|| anyhow::anyhow!("missing outbox row for quiet recipient"))?;
    assert!(quiet_row.recipient_last_request_at.is_none());
    assert!(quiet_row.recipient_last_eligible_turn_at.is_none());

    Ok(())
}

#[tokio::test]
async fn fetch_recipient_activity_returns_timestamps_or_none() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let handle_db = factory.acquire_shared().await?;
    let handle = PersistHandle::connect(&handle_db.database_url).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&handle_db.database_url)
        .await?;

    let (active_uuid, active_id, active_project) = seed_session_with_ids(
        &pool,
        &format!("proj-recipient-activity-active-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let primary_at = Utc::now() - chrono::Duration::minutes(20);
    let subagent_at = Utc::now() - chrono::Duration::minutes(2);
    seed_request_at(&pool, active_id, active_project, "primary", primary_at).await?;
    seed_request_at(&pool, active_id, active_project, "subagent", subagent_at).await?;

    let activity = handle
        .launch()
        .fetch_recipient_activity(active_uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("expected Some for a known session"))?;
    let close = |got: Option<DateTime<Utc>>, want: DateTime<Utc>| {
        got.is_some_and(|t| (t - want).num_seconds().abs() < 2)
    };
    assert!(close(activity.last_request_at, subagent_at));
    assert!(close(activity.last_eligible_turn_at, primary_at));

    let (silent_uuid, _, _) = seed_session_with_ids(
        &pool,
        &format!("proj-recipient-activity-silent-{}", uuid::Uuid::new_v4()),
    )
    .await?;
    let silent_activity = handle
        .launch()
        .fetch_recipient_activity(silent_uuid)
        .await?
        .ok_or_else(|| anyhow::anyhow!("expected Some for a known but silent session"))?;
    assert!(silent_activity.last_request_at.is_none());
    assert!(silent_activity.last_eligible_turn_at.is_none());

    let unknown = handle
        .launch()
        .fetch_recipient_activity(uuid::Uuid::new_v4())
        .await?;
    assert!(unknown.is_none(), "an unknown session must return None");

    Ok(())
}
