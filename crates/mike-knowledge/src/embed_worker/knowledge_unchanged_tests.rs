use super::knowledge::sweep;
use super::knowledge_tests::{breaker, scan, source};
use crate::analysis::{EmbeddingConfig, EmbeddingService};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::{PersistHandle, test_support::acquire_ephemeral_db};
use serde_json::json;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

async fn embedder() -> MockServer {
    let server = MockServer::start().await;
    let mut vector = vec![0.0_f32; 768];
    vector[0] = 1.0;
    Mock::given(method("POST"))
        .and(path("/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [{"embedding": vector}]})))
        .mount(&server)
        .await;
    server
}

async fn calls(server: &MockServer) -> usize {
    server.received_requests().await.map_or(0, |r| r.len())
}

async fn chunk_versions(handle: &PersistHandle) -> anyhow::Result<Vec<String>> {
    Ok(
        sqlx::query_scalar("SELECT xmin::text FROM memory.knowledge_embedding_chunks ORDER BY state_id, ordinal")
            .fetch_all(handle.pool())
            .await?,
    )
}

async fn requeue_everything(handle: &PersistHandle) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE memory.knowledge_items
            SET needs_embedding = true, ready_at = now(), changed_at = clock_timestamp()",
    )
    .execute(handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn a_requeued_item_whose_body_is_unchanged_is_not_embedded_again() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = handle
        .knowledge()
        .define_knowledge_project("Unchanged", Uuid::new_v4())
        .await?;
    let location = source(&handle, &project.project_uuid.to_string()).await?;
    scan(&handle, location, 0, "Evidence that stays the same").await?;
    let server = embedder().await;
    let service = EmbeddingService::new(&EmbeddingConfig {
        enabled: true,
        url: server.uri(),
        model: format!("unchanged-{}", Uuid::new_v4()),
        chunk_max_chars: 80,
    });

    sweep(&handle, &service, &mut breaker()).await?;
    let first_calls = calls(&server).await;
    let first_chunks = chunk_versions(&handle).await?;
    assert!(first_calls > 0 && !first_chunks.is_empty());

    requeue_everything(&handle).await?;
    sweep(&handle, &service, &mut breaker()).await?;
    assert_eq!(calls(&server).await, first_calls, "an unchanged body is not sent to the embedder");
    assert_eq!(chunk_versions(&handle).await?, first_chunks, "its chunks are not rewritten");
    let waiting: i64 =
        sqlx::query_scalar("SELECT count(*) FROM memory.knowledge_items WHERE needs_embedding")
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(waiting, 0, "the item is marked done");

    scan(&handle, location, 1, "Evidence that has changed").await?;
    sweep(&handle, &service, &mut breaker()).await?;
    assert!(calls(&server).await > first_calls, "a changed body is embedded again");
    db.cleanup().await?;
    Ok(())
}
