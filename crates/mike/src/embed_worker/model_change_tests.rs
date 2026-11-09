use super::requeue_on_model_change;
use crate::analysis::{EmbeddingConfig, EmbeddingService};
use crate::store::operator_turns::vector_to_text_literal;
use crate::store::persistence::{PersistHandle, test_support::acquire_ephemeral_db};
use crate::store::test_support::{seed_project, seed_session_turns, seed_two_sessions};

fn service(model: &str) -> EmbeddingService {
    EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        model: model.into(),
        ..EmbeddingConfig::default()
    })
}

async fn embedded_turn(handle: &PersistHandle, model: &str) -> anyhow::Result<i64> {
    let project = seed_project(handle.pool(), &uuid::Uuid::new_v4().to_string()).await?;
    let session = uuid::Uuid::new_v4().to_string();
    seed_two_sessions(
        handle.pool(),
        &session,
        &uuid::Uuid::new_v4().to_string(),
        project,
    )
    .await?;
    seed_session_turns(handle.pool(), &session, project, 1).await?;
    let ids = handle.fetch_unembedded_turn_ids(10).await?;
    let turn_id = *ids
        .first()
        .ok_or_else(|| anyhow::anyhow!("seeded turn missing"))?;
    sqlx::query(
        "INSERT INTO memory.operator_turn_chunks
            (turn_id, chunk_idx, chunk_role_id, chunk_text, chunk_embedding, embedding_model)
         SELECT $1, 0, r.id, 'chunk', $2::public.vector, $3 FROM memory.chunk_roles r WHERE r.code = 'prompt'",
    )
    .bind(turn_id)
    .bind(vector_to_text_literal(&vec![0.5_f32; 768]))
    .bind(model)
    .execute(handle.pool())
    .await?;
    handle.mark_turns_embedded(&[turn_id]).await?;
    Ok(turn_id)
}

async fn is_pending(handle: &PersistHandle, turn_id: i64) -> anyhow::Result<bool> {
    Ok(handle
        .fetch_unembedded_turn_ids(1000)
        .await?
        .contains(&turn_id))
}

#[tokio::test]
async fn requeue_on_model_change_with_chunks_from_another_model_queues_the_turn()
-> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let stale = embedded_turn(&handle, "nomic-embed-text-v1.5").await?;
    let current = embedded_turn(&handle, "embeddinggemma").await?;
    assert!(!is_pending(&handle, stale).await?);

    let mut requeued_for = None;
    requeue_on_model_change(&handle, &service("embeddinggemma"), &mut requeued_for).await;

    assert!(is_pending(&handle, stale).await?);
    assert!(!is_pending(&handle, current).await?);
    assert_eq!(requeued_for.as_deref(), Some("embeddinggemma"));
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn requeue_on_model_change_with_the_same_model_twice_queues_nothing_more()
-> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let mut requeued_for = None;
    requeue_on_model_change(&handle, &service("embeddinggemma"), &mut requeued_for).await;
    let stale = embedded_turn(&handle, "nomic-embed-text-v1.5").await?;

    requeue_on_model_change(&handle, &service("embeddinggemma"), &mut requeued_for).await;

    assert!(!is_pending(&handle, stale).await?);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn requeue_on_model_change_with_embedding_disabled_queues_nothing() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let stale = embedded_turn(&handle, "nomic-embed-text-v1.5").await?;
    let disabled = EmbeddingService::new(&EmbeddingConfig::default());
    let mut requeued_for = None;

    requeue_on_model_change(&handle, &disabled, &mut requeued_for).await;

    assert!(!is_pending(&handle, stale).await?);
    assert_eq!(requeued_for, None);
    db.cleanup().await?;
    Ok(())
}
