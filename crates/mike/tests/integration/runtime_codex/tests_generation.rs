use uuid::Uuid;

use mike::runtime::codex::TextChannel;
use mike::store::persistence::runtime::RuntimeWriteOutcome;
use mike::store::persistence::{PersistError, PersistHandle};

use super::test_support::{captured, database, delta, input, key, page};
use super::{CodexProjectionApplication, CodexProjectionApplyOutcome};

async fn apply(
    handle: &PersistHandle,
    generation_id: Uuid,
) -> anyhow::Result<CodexProjectionApplication> {
    match handle.apply_next_codex_projection(generation_id).await? {
        CodexProjectionApplyOutcome::Applied(application) => Ok(application),
        CodexProjectionApplyOutcome::CaughtUp(_) => anyhow::bail!("expected an application"),
    }
}

async fn assert_text(
    handle: &PersistHandle,
    generation_id: Uuid,
    namespace_id: Uuid,
    expected: &str,
) -> anyhow::Result<()> {
    let item = handle
        .codex_projected_item(generation_id, &key(namespace_id))
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing projected item"))?;
    assert_eq!(
        item.item.display_text.get(&TextChannel::Message),
        Some(&expected.to_owned())
    );
    assert_eq!(item.item.display_text, item.item.streamed_text);
    Ok(())
}

async fn assert_retired(handle: &PersistHandle, generation_id: Uuid) -> anyhow::Result<()> {
    assert_eq!(
        handle
            .codex_projection_generation(generation_id)
            .await?
            .map(|generation| generation.state),
        Some("retired".into())
    );
    assert!(matches!(
        handle.apply_next_codex_projection(generation_id).await,
        Err(PersistError::DataInvariant(_))
    ));
    assert!(matches!(
        handle
            .activate_codex_projection_generation(generation_id)
            .await,
        Err(PersistError::DataInvariant(_))
    ));
    Ok(())
}

#[tokio::test]
async fn generations_replay_receipts_independently_and_isolate_namespaces() -> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let mut foreign_namespace = namespace.clone();
    foreign_namespace.id = Uuid::new_v4();
    foreign_namespace.durable_key = format!("fixture/{}", foreign_namespace.id);
    handle
        .register_runtime_namespace(&foreign_namespace)
        .await?;
    for namespace_id in [namespace.id, namespace.id, foreign_namespace.id] {
        captured(
            handle
                .capture_codex_receipt(input(namespace_id, delta("x"))?)
                .await?,
        )?;
    }
    let first = Uuid::new_v4();
    let replay = Uuid::new_v4();
    let foreign = Uuid::new_v4();
    for (namespace_id, generation_id) in [
        (namespace.id, first),
        (namespace.id, replay),
        (foreign_namespace.id, foreign),
    ] {
        handle
            .begin_codex_projection_generation(namespace_id, generation_id)
            .await?;
    }
    assert_eq!(apply(&handle, first).await?.capture_sequence, 1);
    assert!(
        handle
            .codex_projected_item(replay, &key(namespace.id))
            .await?
            .is_none()
    );
    assert_eq!(apply(&handle, replay).await?.capture_sequence, 1);
    assert_eq!(apply(&handle, replay).await?.capture_sequence, 2);
    assert_text(&handle, first, namespace.id, "x").await?;
    assert_text(&handle, replay, namespace.id, "xx").await?;
    assert_eq!(apply(&handle, first).await?.capture_sequence, 2);
    assert_eq!(
        handle
            .codex_projected_item(first, &key(namespace.id))
            .await?,
        handle
            .codex_projected_item(replay, &key(namespace.id))
            .await?
    );
    assert_eq!(
        handle.list_codex_projection_effects(page(first)).await?,
        handle.list_codex_projection_effects(page(replay)).await?
    );
    assert_eq!(apply(&handle, foreign).await?.capture_sequence, 1);
    assert_text(&handle, foreign, foreign_namespace.id, "x").await?;
    assert!(
        handle
            .codex_projected_item(foreign, &key(namespace.id))
            .await?
            .is_none()
    );
    assert!(matches!(
        handle
            .begin_codex_projection_generation(foreign_namespace.id, first)
            .await,
        Err(PersistError::DataInvariant(_))
    ));
    assert!(matches!(
        handle.apply_next_codex_projection(first).await?,
        CodexProjectionApplyOutcome::CaughtUp(generation)
            if generation.last_applied_capture_sequence == 2
    ));
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_generation_application_applies_each_receipt_exactly_once() -> anyhow::Result<()>
{
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let receipt = captured(
        handle
            .capture_codex_receipt(input(namespace.id, delta("once"))?)
            .await?,
    )?;
    let generation_id = Uuid::new_v4();
    handle
        .begin_codex_projection_generation(namespace.id, generation_id)
        .await?;
    let (first, second) = tokio::join!(
        handle.apply_next_codex_projection(generation_id),
        handle.apply_next_codex_projection(generation_id),
    );
    let outcomes = [first?, second?];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, CodexProjectionApplyOutcome::Applied(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, CodexProjectionApplyOutcome::CaughtUp(_)))
            .count(),
        1
    );
    let application = handle
        .codex_projection_application(generation_id, receipt.event_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing application"))?;
    assert_eq!(application.effect_count, 1);
    assert_eq!(application.capture_sequence, 1);
    assert_text(&handle, generation_id, namespace.id, "once").await?;
    assert_eq!(
        handle
            .list_codex_projection_effects(page(generation_id))
            .await?
            .len(),
        1
    );
    assert!(matches!(
        handle.apply_next_codex_projection(generation_id).await?,
        CodexProjectionApplyOutcome::CaughtUp(_)
    ));
    assert_eq!(
        handle
            .codex_projection_application(generation_id, receipt.event_id)
            .await?,
        Some(application)
    );
    assert_text(&handle, generation_id, namespace.id, "once").await?;
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn generation_promotion_requires_catchup_and_preserves_active_head_on_failure()
-> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let active = Uuid::new_v4();
    handle
        .begin_codex_projection_generation(namespace.id, active)
        .await?;
    assert!(
        handle
            .active_codex_projection_generation(namespace.id)
            .await?
            .is_none()
    );
    assert_eq!(
        handle.activate_codex_projection_generation(active).await?,
        RuntimeWriteOutcome::Inserted
    );
    captured(
        handle
            .capture_codex_receipt(input(namespace.id, delta("a"))?)
            .await?,
    )?;
    apply(&handle, active).await?;
    let rebuilding = Uuid::new_v4();
    handle
        .begin_codex_projection_generation(namespace.id, rebuilding)
        .await?;
    apply(&handle, rebuilding).await?;
    captured(
        handle
            .capture_codex_receipt(input(namespace.id, delta("b"))?)
            .await?,
    )?;
    let old_head = handle
        .active_codex_projection_generation(namespace.id)
        .await?;
    assert!(matches!(
        handle
            .activate_codex_projection_generation(rebuilding)
            .await,
        Err(PersistError::DataInvariant(_))
    ));
    assert_eq!(
        handle
            .active_codex_projection_generation(namespace.id)
            .await?,
        old_head
    );
    let building = handle
        .codex_projection_generation(rebuilding)
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing building generation"))?;
    assert_eq!(building.state, "building");
    assert_eq!(building.last_applied_capture_sequence, 1);
    apply(&handle, rebuilding).await?;
    assert_eq!(
        handle
            .activate_codex_projection_generation(rebuilding)
            .await?,
        RuntimeWriteOutcome::Inserted
    );
    let new_head = handle
        .active_codex_projection_generation(namespace.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("missing active generation"))?;
    assert_eq!(new_head.generation_id, rebuilding);
    assert_eq!(new_head.state, "active");
    assert_eq!(new_head.last_applied_capture_sequence, 2);
    assert_eq!(
        handle
            .activate_codex_projection_generation(rebuilding)
            .await?,
        RuntimeWriteOutcome::Unchanged
    );
    assert_retired(&handle, active).await?;
    assert_eq!(
        handle
            .active_codex_projection_generation(namespace.id)
            .await?,
        Some(new_head)
    );
    assert_text(&handle, active, namespace.id, "a").await?;
    assert_text(&handle, rebuilding, namespace.id, "ab").await?;
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn unknown_generation_versions_reject_reads_writes_and_promotion() -> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let active = Uuid::new_v4();
    handle
        .begin_codex_projection_generation(namespace.id, active)
        .await?;
    handle.activate_codex_projection_generation(active).await?;
    let receipt = captured(
        handle
            .capture_codex_receipt(input(namespace.id, delta("v"))?)
            .await?,
    )?;
    let head = handle
        .active_codex_projection_generation(namespace.id)
        .await?;
    for (projector, codec) in [(99, 1), (1, 99)] {
        let generation_id = Uuid::new_v4();
        handle
            .begin_codex_projection_generation(namespace.id, generation_id)
            .await?;
        sqlx::query("UPDATE runtime.codex_projection_generations SET projector_version = $2, state_codec_version = $3 WHERE generation_id = $1")
            .bind(generation_id).bind(projector).bind(codec).execute(handle.pool()).await?;
        assert!(matches!(
            handle
                .begin_codex_projection_generation(namespace.id, generation_id)
                .await,
            Err(PersistError::DataInvariant(_))
        ));
        assert!(matches!(
            handle.apply_next_codex_projection(generation_id).await,
            Err(PersistError::DataInvariant(_))
        ));
        assert!(matches!(
            handle
                .activate_codex_projection_generation(generation_id)
                .await,
            Err(PersistError::DataInvariant(_))
        ));
        assert!(matches!(
            handle
                .codex_projected_item(generation_id, &key(namespace.id))
                .await,
            Err(PersistError::DataInvariant(_))
        ));
        assert!(matches!(
            handle
                .list_codex_projection_effects(page(generation_id))
                .await,
            Err(PersistError::DataInvariant(_))
        ));
        assert!(matches!(
            handle
                .codex_projection_application(generation_id, receipt.event_id)
                .await,
            Err(PersistError::DataInvariant(_))
        ));
        assert!(matches!(
            handle
                .list_codex_projection_issues(generation_id, 0, 100)
                .await,
            Err(PersistError::DataInvariant(_))
        ));
        assert_eq!(
            handle
                .active_codex_projection_generation(namespace.id)
                .await?,
            head
        );
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM runtime.codex_projection_applications WHERE generation_id = $1",
        )
        .bind(generation_id)
        .fetch_one(handle.pool())
        .await?;
        assert_eq!(count, 0);
    }
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn unknown_stored_item_codec_rejects_read_and_rolls_back_next_application()
-> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let generation_id = Uuid::new_v4();
    handle
        .begin_codex_projection_generation(namespace.id, generation_id)
        .await?;
    captured(
        handle
            .capture_codex_receipt(input(namespace.id, delta("a"))?)
            .await?,
    )?;
    apply(&handle, generation_id).await?;
    let second = captured(
        handle
            .capture_codex_receipt(input(namespace.id, delta("b"))?)
            .await?,
    )?;
    sqlx::query(
        "UPDATE runtime.codex_item_state SET state_codec_version = 99 WHERE generation_id = $1",
    )
    .bind(generation_id)
    .execute(handle.pool())
    .await?;
    assert!(matches!(
        handle
            .codex_projected_item(generation_id, &key(namespace.id))
            .await,
        Err(PersistError::DataInvariant(_))
    ));
    assert!(matches!(
        handle.apply_next_codex_projection(generation_id).await,
        Err(PersistError::DataInvariant(_))
    ));
    assert!(
        handle
            .codex_projection_application(generation_id, second.event_id)
            .await?
            .is_none()
    );
    assert_eq!(
        handle
            .list_codex_projection_effects(page(generation_id))
            .await?
            .len(),
        1
    );
    assert_eq!(
        handle
            .codex_projection_generation(generation_id)
            .await?
            .map(|generation| generation.last_applied_capture_sequence),
        Some(1)
    );
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
