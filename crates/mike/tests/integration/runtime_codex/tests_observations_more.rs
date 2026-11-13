//! The rest of tests_observations's tests.

use super::*;

#[tokio::test]
async fn history_duplicate_item_effects_keep_ordinals_and_reduce_in_source_order()
-> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let generation_id = Uuid::new_v4();
    handle
        .runtime()
        .begin_codex_projection_generation(namespace.id, generation_id)
        .await?;
    let (receipt, application) = capture_and_apply(
        &handle,
        generation_id,
        rpc_input(namespace.id, "thread/items/list", history_result())?,
    )
    .await?;
    assert_eq!(application.effect_count, 3);
    assert_eq!(application.status, ProjectionStatus::Projected);
    let effects = handle
        .runtime()
        .list_codex_projection_effects(page(generation_id))
        .await?;
    assert_eq!(
        effects
            .iter()
            .map(|effect| effect.effect_ordinal)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert!(
        effects
            .iter()
            .all(|effect| effect.event_id == receipt.event_id && effect.capture_sequence == 1)
    );
    let mut following = page(generation_id);
    following.after = Some((1, 0));
    assert_eq!(
        handle
            .runtime()
            .list_codex_projection_effects(following)
            .await?,
        effects[1..]
    );
    assert_history_items(&handle, generation_id, namespace.id).await?;
    assert!(matches!(
        handle
            .runtime()
            .apply_next_codex_projection(generation_id)
            .await?,
        CodexProjectionApplyOutcome::CaughtUp(_)
    ));
    let replay = Uuid::new_v4();
    handle
        .runtime()
        .begin_codex_projection_generation(namespace.id, replay)
        .await?;
    assert!(matches!(
        handle.runtime().apply_next_codex_projection(replay).await?,
        CodexProjectionApplyOutcome::Applied(_)
    ));
    assert_eq!(
        handle
            .runtime()
            .list_codex_projection_effects(page(replay))
            .await?,
        effects
    );
    assert_history_items(&handle, replay, namespace.id).await?;
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
