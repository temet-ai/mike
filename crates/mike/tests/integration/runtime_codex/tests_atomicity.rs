use std::time::Duration;

use uuid::Uuid;

use mike::runtime::RuntimeObservation;
use mike::runtime::codex::TextChannel;

use super::test_support::{captured, count, database, delta, input, key, receipt_id};
use super::{CodexProjectionApplyOutcome, RawEventPage};
use mike::store::persistence::runtime::codex::test_seams::{
    insert_context, insert_observation, lock_intake,
};

#[tokio::test]
async fn context_write_failure_rolls_back_raw_event_and_intake_counter() -> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    sqlx::query("ALTER TABLE runtime.codex_receipt_context ADD CONSTRAINT fixture_reject_context CHECK (context_version <> 1)")
        .execute(handle.pool()).await?;
    let request = input(namespace.id, delta("A"))?;
    assert!(handle.capture_codex_receipt(request.clone()).await.is_err());
    assert!(
        handle
            .runtime_receipt(receipt_id(&request))
            .await?
            .is_none()
    );
    assert_eq!(count(&handle, namespace.id, "events").await?, 0);
    assert_eq!(count(&handle, namespace.id, "context").await?, 0);
    sqlx::query("ALTER TABLE runtime.codex_receipt_context DROP CONSTRAINT fixture_reject_context")
        .execute(handle.pool())
        .await?;
    let receipt = captured(handle.capture_codex_receipt(request).await?)?;
    assert_eq!(receipt.capture_sequence, Some(1));
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn application_failure_rolls_back_effects_state_and_marker_but_preserves_raw()
-> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let request = input(namespace.id, delta("A\0B"))?;
    let raw = captured(handle.capture_codex_receipt(request.clone()).await?)?;
    let generation = handle
        .begin_codex_projection_generation(namespace.id, Uuid::new_v4())
        .await?;
    sqlx::query("ALTER TABLE runtime.codex_projection_applications ADD CONSTRAINT fixture_reject_application CHECK (effect_count < 0)")
        .execute(handle.pool()).await?;
    assert!(
        handle
            .apply_next_codex_projection(generation.generation_id)
            .await
            .is_err()
    );
    for table in ["effects", "items", "applications"] {
        assert_eq!(count(&handle, namespace.id, table).await?, 0);
    }
    assert_eq!(
        handle.runtime_receipt(receipt_id(&request)).await?,
        Some(raw)
    );
    assert_eq!(
        handle
            .codex_projection_generation(generation.generation_id)
            .await?,
        Some(generation.clone())
    );
    sqlx::query("ALTER TABLE runtime.codex_projection_applications DROP CONSTRAINT fixture_reject_application")
        .execute(handle.pool()).await?;
    assert!(matches!(
        handle
            .apply_next_codex_projection(generation.generation_id)
            .await?,
        CodexProjectionApplyOutcome::Applied(_)
    ));
    let state = handle
        .codex_projected_item(generation.generation_id, &key(namespace.id))
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing item"))?;
    assert_eq!(
        state
            .item
            .display_text
            .get(&TextChannel::Message)
            .map(String::as_str),
        Some("A\0B")
    );
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn intake_lock_hides_uncommitted_sequence_and_preserves_delta_order() -> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let generation = handle
        .begin_codex_projection_generation(namespace.id, Uuid::new_v4())
        .await?;
    let first = input(namespace.id, delta("A"))?;
    let observation = RuntimeObservation::new(first.envelope, first.raw_payload)?;
    let mut transaction = handle.pool().begin().await?;
    lock_intake(&mut transaction, namespace.id).await?;
    let first_id = insert_observation(&mut transaction, &observation).await?;
    assert_eq!(
        insert_context(
            &mut transaction,
            first_id,
            &observation,
            &first.context,
            false
        )
        .await?,
        1
    );
    let second_input = input(namespace.id, delta("B"))?;
    let second = handle.capture_codex_receipt(second_input);
    tokio::pin!(second);
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut second)
            .await
            .is_err()
    );
    assert!(matches!(
        handle
            .apply_next_codex_projection(generation.generation_id)
            .await?,
        CodexProjectionApplyOutcome::CaughtUp(_)
    ));
    transaction.commit().await?;
    let second = captured(second.await?)?;
    assert_eq!(second.capture_sequence, Some(2));
    assert!(first_id < second.event_id);
    handle
        .apply_next_codex_projection(generation.generation_id)
        .await?;
    handle
        .apply_next_codex_projection(generation.generation_id)
        .await?;
    let state = handle
        .codex_projected_item(generation.generation_id, &key(namespace.id))
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing item"))?;
    assert_eq!(
        state
            .item
            .display_text
            .get(&TextChannel::Message)
            .map(String::as_str),
        Some("AB")
    );
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn late_lower_raw_event_stays_visible_and_cannot_break_capture_checkpoint()
-> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let legacy = input(namespace.id, delta("legacy"))?;
    let raw = RuntimeObservation::new(legacy.envelope, legacy.raw_payload)?;
    let mut transaction = handle.pool().begin().await?;
    let legacy_id = insert_observation(&mut transaction, &raw).await?;
    let eligible = captured(
        handle
            .capture_codex_receipt(input(namespace.id, delta("A"))?)
            .await?,
    )?;
    assert!(legacy_id < eligible.event_id);
    let generation = handle
        .begin_codex_projection_generation(namespace.id, Uuid::new_v4())
        .await?;
    handle
        .apply_next_codex_projection(generation.generation_id)
        .await?;
    transaction.commit().await?;
    let following = captured(
        handle
            .capture_codex_receipt(input(namespace.id, delta("B"))?)
            .await?,
    )?;
    assert_eq!(following.capture_sequence, Some(2));
    handle
        .apply_next_codex_projection(generation.generation_id)
        .await?;
    let unclassified = handle
        .list_codex_unclassified_receipts(RawEventPage {
            namespace_id: namespace.id,
            after_event_id: 0,
            limit: 10,
        })
        .await?;
    assert_eq!(unclassified.len(), 1);
    assert_eq!(unclassified[0].event_id, legacy_id);
    let state = handle
        .codex_projected_item(generation.generation_id, &key(namespace.id))
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing item"))?;
    assert_eq!(
        state
            .item
            .display_text
            .get(&TextChannel::Message)
            .map(String::as_str),
        Some("AB")
    );
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn intake_during_promotion_blocks_activation_until_its_receipt_is_applied()
-> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let generation = handle
        .begin_codex_projection_generation(namespace.id, Uuid::new_v4())
        .await?;
    let first = input(namespace.id, delta("A"))?;
    let observation = RuntimeObservation::new(first.envelope, first.raw_payload)?;
    let mut transaction = handle.pool().begin().await?;
    lock_intake(&mut transaction, namespace.id).await?;
    let event_id = insert_observation(&mut transaction, &observation).await?;
    insert_context(
        &mut transaction,
        event_id,
        &observation,
        &first.context,
        false,
    )
    .await?;
    let activation = handle.activate_codex_projection_generation(generation.generation_id);
    tokio::pin!(activation);
    assert!(
        tokio::time::timeout(Duration::from_millis(40), &mut activation)
            .await
            .is_err()
    );
    transaction.commit().await?;
    assert!(activation.await.is_err());
    assert!(
        handle
            .active_codex_projection_generation(namespace.id)
            .await?
            .is_none()
    );
    handle
        .apply_next_codex_projection(generation.generation_id)
        .await?;
    handle
        .activate_codex_projection_generation(generation.generation_id)
        .await?;
    assert_eq!(
        handle
            .active_codex_projection_generation(namespace.id)
            .await?
            .map(|value| value.generation_id),
        Some(generation.generation_id)
    );
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
