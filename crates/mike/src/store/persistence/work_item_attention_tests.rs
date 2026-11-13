use crate::store::persistence::launch_repo::LaunchPersist as _;
use chrono::Utc;
use uuid::Uuid;

use super::PersistHandle;
use super::test_support::acquire_ephemeral_db;
use crate::work_items::attention::Attention;

fn attention(subject: &str) -> Attention {
    Attention {
        kind: "stalled_wait",
        subject: subject.to_owned(),
        since: Utc::now(),
        message: format!("{subject} has not moved"),
    }
}

async fn mailbox_rows(persist: &PersistHandle, message_uuid: Uuid) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM mailbox.messages WHERE message_uuid = $1"#,
        message_uuid,
    )
    .fetch_one(persist.pool())
    .await?)
}

#[tokio::test]
async fn a_recorded_but_undelivered_item_comes_back_on_the_next_pass() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let session = persist
        .launch()
        .ensure_session_exists(&Uuid::new_v4().to_string())
        .await?;
    let item = attention("job1@100");

    let first = persist
        .launch()
        .record_work_item_attention(session, &item)
        .await?
        .ok_or_else(|| anyhow::anyhow!("first record returned nothing"))?;
    assert!(first.inserted);
    assert!(!first.delivered);
    assert_eq!(first.delivery_attempts, 0);

    let again = persist
        .launch()
        .record_work_item_attention(session, &item)
        .await?
        .ok_or_else(|| anyhow::anyhow!("second record returned nothing"))?;
    assert!(!again.inserted, "the row already existed");
    assert!(!again.delivered, "and it was never delivered");
    assert_eq!(again.id, first.id);

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn delivery_inserts_the_message_and_marks_the_item_in_one_step() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let session = persist
        .launch()
        .ensure_session_exists(&Uuid::new_v4().to_string())
        .await?;
    let item = attention("job1@100");
    let recorded = persist
        .launch()
        .record_work_item_attention(session, &item)
        .await?
        .ok_or_else(|| anyhow::anyhow!("record returned nothing"))?;
    let message_uuid = Uuid::from_u128(0xfeed);

    assert!(
        persist
            .launch()
            .deliver_work_item_attention(recorded.id, message_uuid, "[mike work registry] stuck")
            .await?
    );
    assert_eq!(mailbox_rows(&persist, message_uuid).await?, 1);
    let after = persist
        .launch()
        .record_work_item_attention(session, &item)
        .await?
        .ok_or_else(|| anyhow::anyhow!("record returned nothing"))?;
    assert!(after.delivered);

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_retry_after_a_lost_marker_does_not_send_the_mail_twice() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let session = persist
        .launch()
        .ensure_session_exists(&Uuid::new_v4().to_string())
        .await?;
    let item = attention("job1@100");
    let recorded = persist
        .launch()
        .record_work_item_attention(session, &item)
        .await?
        .ok_or_else(|| anyhow::anyhow!("record returned nothing"))?;
    let message_uuid = Uuid::from_u128(0xbeef);

    sqlx::query!(
        "INSERT INTO mailbox.messages (message_uuid, to_session_id, from_session_id, body)
         VALUES ($1, $2, NULL, 'already sent')",
        message_uuid,
        session,
    )
    .execute(persist.pool())
    .await?;

    assert!(
        persist
            .launch()
            .deliver_work_item_attention(recorded.id, message_uuid, "[mike work registry] stuck")
            .await?,
        "the item is marked against the mail that already exists"
    );
    assert_eq!(
        mailbox_rows(&persist, message_uuid).await?,
        1,
        "no second message"
    );
    assert!(
        persist
            .launch()
            .deliver_work_item_attention(recorded.id, message_uuid, "[mike work registry] stuck")
            .await?,
        "a repeat is harmless"
    );
    assert_eq!(mailbox_rows(&persist, message_uuid).await?, 1);

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn delivery_to_an_item_that_is_gone_sends_nothing_and_says_so() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let message_uuid = Uuid::from_u128(0xdead);

    assert!(
        !persist
            .launch()
            .deliver_work_item_attention(987_654, message_uuid, "[mike work registry] gone")
            .await?
    );
    assert_eq!(mailbox_rows(&persist, message_uuid).await?, 0);

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_failed_delivery_is_counted_against_the_item() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let session = persist
        .launch()
        .ensure_session_exists(&Uuid::new_v4().to_string())
        .await?;
    let item = attention("job1@100");
    let recorded = persist
        .launch()
        .record_work_item_attention(session, &item)
        .await?
        .ok_or_else(|| anyhow::anyhow!("record returned nothing"))?;

    persist
        .launch()
        .note_work_item_attention_delivery_failed(recorded.id, 6, "mailbox down")
        .await?;
    persist
        .launch()
        .note_work_item_attention_delivery_failed(recorded.id, 6, "mailbox down")
        .await?;

    let after = persist
        .launch()
        .record_work_item_attention(session, &item)
        .await?
        .ok_or_else(|| anyhow::anyhow!("record returned nothing"))?;
    assert_eq!(after.delivery_attempts, 2);
    assert!(after.last_delivery_attempt_at.is_some());
    assert!(!after.delivered);
    assert_eq!(after.disposition, "pending");

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn repeated_passes_mail_once_and_a_give_up_stays_visible() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let session = persist
        .launch()
        .ensure_session_exists(&Uuid::new_v4().to_string())
        .await?;
    let flaky = attention("flaky@1");
    let hopeless = attention("hopeless@1");
    let message_uuid = Uuid::from_u128(0xf1a);

    for pass in 0..5 {
        let recorded = persist
            .launch()
            .record_work_item_attention(session, &flaky)
            .await?
            .ok_or_else(|| anyhow::anyhow!("flaky returned nothing"))?;
        if recorded.delivered {
            continue;
        }
        if pass < 2 {
            persist
                .launch()
                .note_work_item_attention_delivery_failed(recorded.id, 3, "mailbox down")
                .await?;
        } else {
            persist
                .launch()
                .deliver_work_item_attention(
                    recorded.id,
                    message_uuid,
                    "[mike work registry] flaky",
                )
                .await?;
        }
    }
    assert_eq!(mailbox_rows(&persist, message_uuid).await?, 1);
    let flaky_row = persist
        .launch()
        .record_work_item_attention(session, &flaky)
        .await?
        .ok_or_else(|| anyhow::anyhow!("flaky returned nothing"))?;
    assert_eq!(flaky_row.disposition, "delivered");
    assert_eq!(flaky_row.delivery_attempts, 2);

    for _ in 0..4 {
        let recorded = persist
            .launch()
            .record_work_item_attention(session, &hopeless)
            .await?
            .ok_or_else(|| anyhow::anyhow!("hopeless returned nothing"))?;
        if recorded.delivery_attempts < 3 {
            persist
                .launch()
                .note_work_item_attention_delivery_failed(recorded.id, 3, "mailbox down")
                .await?;
        }
    }
    let given_up = persist
        .launch()
        .record_work_item_attention(session, &hopeless)
        .await?
        .ok_or_else(|| anyhow::anyhow!("hopeless returned nothing"))?;
    assert_eq!(given_up.disposition, "given_up");
    assert_eq!(given_up.delivery_attempts, 3);
    let reason = sqlx::query_scalar!(
        "SELECT disposition_reason FROM agents.work_item_attention WHERE id = $1",
        given_up.id,
    )
    .fetch_one(persist.pool())
    .await?;
    assert_eq!(reason.as_deref(), Some("mailbox down"));

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_stale_or_resolved_item_keeps_its_disposition_and_reason() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let session = persist
        .launch()
        .ensure_session_exists(&Uuid::new_v4().to_string())
        .await?;
    let item = attention("old@1");
    let recorded = persist
        .launch()
        .record_work_item_attention(session, &item)
        .await?
        .ok_or_else(|| anyhow::anyhow!("record returned nothing"))?;

    persist
        .launch()
        .settle_work_item_attention(recorded.id, "stale", "too old")
        .await?;
    persist
        .launch()
        .settle_work_item_attention(recorded.id, "resolved", "ignored: already settled")
        .await?;

    let after = persist
        .launch()
        .record_work_item_attention(session, &item)
        .await?
        .ok_or_else(|| anyhow::anyhow!("record returned nothing"))?;
    assert_eq!(after.disposition, "stale");
    assert!(!after.delivered);

    db.cleanup().await?;
    Ok(())
}
