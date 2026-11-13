use super::*;

#[tokio::test]
async fn unknown_stored_item_codec_rejects_read_and_rolls_back_next_application()
-> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let generation_id = Uuid::new_v4();
    handle
        .runtime()
        .begin_codex_projection_generation(namespace.id, generation_id)
        .await?;
    captured(
        handle
            .runtime()
            .capture_codex_receipt(input(namespace.id, delta("a"))?)
            .await?,
    )?;
    apply(&handle, generation_id).await?;
    let second = captured(
        handle
            .runtime()
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
            .runtime()
            .codex_projected_item(generation_id, &key(namespace.id))
            .await,
        Err(PersistError::DataInvariant(_))
    ));
    assert!(matches!(
        handle
            .runtime()
            .apply_next_codex_projection(generation_id)
            .await,
        Err(PersistError::DataInvariant(_))
    ));
    assert!(
        handle
            .runtime()
            .codex_projection_application(generation_id, second.event_id)
            .await?
            .is_none()
    );
    assert_eq!(
        handle
            .runtime()
            .list_codex_projection_effects(page(generation_id))
            .await?
            .len(),
        1
    );
    assert_eq!(
        handle
            .runtime()
            .codex_projection_generation(generation_id)
            .await?
            .map(|generation| generation.last_applied_capture_sequence),
        Some(1)
    );
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
