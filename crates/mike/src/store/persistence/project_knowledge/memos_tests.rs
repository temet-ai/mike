use std::time::Duration;

use serde_json::Value;
use uuid::Uuid;

use super::memos::{COVERAGE_LIFETIME, NEAREST_FIRST_LIFETIME, NO_PROJECT_LIFETIME};
use super::{KnowledgeEmbeddingSource, ModelKeys};
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db, seed_project};
use crate::store::persistence::{PersistHandle, SENTINEL_PROJECT_PATH};

struct Fixture {
    db: TestDbHandle,
    handle: PersistHandle,
}

async fn fixture() -> anyhow::Result<Fixture> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Fixture { db, handle })
}

async fn project(handle: &PersistHandle, name: &str) -> anyhow::Result<i64> {
    Ok(handle
        .define_knowledge_project(name, Uuid::new_v4())
        .await?
        .id)
}

#[tokio::test]
async fn knowledge_coverage_two_calls_inside_the_lifetime_run_the_count_once() -> anyhow::Result<()>
{
    let fx = fixture().await?;
    let id = project(&fx.handle, "Coverage memo").await?;

    let first = fx
        .handle
        .knowledge_coverage(id, &ModelKeys::only("model-a"))
        .await?;
    let second = fx
        .handle
        .knowledge_coverage(id, &ModelKeys::only("model-a"))
        .await?;

    assert_eq!(first, second);
    assert_eq!(fx.handle.knowledge_memos().counts_fetched().1, 1);
    fx.db.cleanup().await
}

#[tokio::test]
async fn knowledge_coverage_figures_equal_the_uncached_count() -> anyhow::Result<()> {
    let fx = fixture().await?;
    let id = project(&fx.handle, "Coverage figures").await?;

    let remembered = fx
        .handle
        .knowledge_coverage(id, &ModelKeys::only("model-a"))
        .await?;
    let direct: Value = fx
        .handle
        .knowledge_embedding_health(Some(id), &ModelKeys::only("model-a"))
        .await?;

    assert_eq!(remembered, direct);
    fx.db.cleanup().await
}

#[tokio::test]
async fn knowledge_coverage_after_the_lifetime_runs_the_count_again() -> anyhow::Result<()> {
    let fx = fixture().await?;
    let id = project(&fx.handle, "Coverage expiry").await?;
    fx.handle
        .knowledge_coverage(id, &ModelKeys::only("model-a"))
        .await?;

    fx.handle
        .knowledge_memos()
        .advance_all(COVERAGE_LIFETIME + Duration::from_secs(1));
    fx.handle
        .knowledge_coverage(id, &ModelKeys::only("model-a"))
        .await?;

    assert_eq!(fx.handle.knowledge_memos().counts_fetched().1, 2);
    fx.db.cleanup().await
}

#[tokio::test]
async fn knowledge_coverage_after_an_embedding_is_recorded_is_counted_again() -> anyhow::Result<()>
{
    let fx = fixture().await?;
    let id = project(&fx.handle, "Coverage invalidation").await?;
    fx.handle
        .knowledge_coverage(id, &ModelKeys::only("model-a"))
        .await?;
    let source = KnowledgeEmbeddingSource {
        reference: "artifact:1".into(),
        source_kind: "artifact".into(),
        source_id: 1,
        source_updated_at: None,
        source_seq: None,
        source_hash: None,
        header: "artifact".into(),
        body: "text".into(),
    };

    fx.handle
        .fail_knowledge_embedding(&source, "model-a", "embedder down")
        .await?;
    fx.handle
        .knowledge_coverage(id, &ModelKeys::only("model-a"))
        .await?;

    assert_eq!(fx.handle.knowledge_memos().counts_fetched().1, 2);
    fx.db.cleanup().await
}

#[tokio::test]
async fn knowledge_coverage_for_another_model_or_project_is_counted_separately()
-> anyhow::Result<()> {
    let fx = fixture().await?;
    let first = project(&fx.handle, "Coverage one").await?;
    let second = project(&fx.handle, "Coverage two").await?;

    fx.handle
        .knowledge_coverage(first, &ModelKeys::only("model-a"))
        .await?;
    fx.handle
        .knowledge_coverage(first, &ModelKeys::only("model-b"))
        .await?;
    fx.handle
        .knowledge_coverage(second, &ModelKeys::only("model-a"))
        .await?;

    assert_eq!(fx.handle.knowledge_memos().counts_fetched().1, 3);
    fx.db.cleanup().await
}

#[tokio::test]
async fn knowledge_nearest_first_two_calls_inside_the_lifetime_run_the_count_once()
-> anyhow::Result<()> {
    let fx = fixture().await?;
    let id = project(&fx.handle, "Nearest memo").await?;

    let first = fx
        .handle
        .knowledge_nearest_first(id, &ModelKeys::only("model-a"))
        .await?;
    let second = fx
        .handle
        .knowledge_nearest_first(id, &ModelKeys::only("model-a"))
        .await?;

    assert_eq!(first, second);
    assert_eq!(fx.handle.knowledge_memos().counts_fetched().0, 1);
    fx.db.cleanup().await
}

#[tokio::test]
async fn knowledge_nearest_first_after_its_lifetime_runs_the_count_again() -> anyhow::Result<()> {
    let fx = fixture().await?;
    let id = project(&fx.handle, "Nearest expiry").await?;
    fx.handle
        .knowledge_nearest_first(id, &ModelKeys::only("model-a"))
        .await?;

    fx.handle
        .knowledge_memos()
        .advance_all(NEAREST_FIRST_LIFETIME - Duration::from_secs(1));
    fx.handle
        .knowledge_nearest_first(id, &ModelKeys::only("model-a"))
        .await?;
    assert_eq!(fx.handle.knowledge_memos().counts_fetched().0, 1);

    fx.handle
        .knowledge_memos()
        .advance_all(Duration::from_secs(2));
    fx.handle
        .knowledge_nearest_first(id, &ModelKeys::only("model-a"))
        .await?;
    assert_eq!(fx.handle.knowledge_memos().counts_fetched().0, 2);
    fx.db.cleanup().await
}

#[tokio::test]
async fn no_project_knowledge_project_is_looked_up_once_while_it_exists() -> anyhow::Result<()> {
    let fx = fixture().await?;
    let wanted = seed_project(fx.handle.pool(), SENTINEL_PROJECT_PATH).await?;

    let first = fx.handle.no_project_knowledge_project().await?;
    let second = fx.handle.no_project_knowledge_project().await?;

    assert_eq!(first.map(|found| found.id), Some(wanted));
    assert_eq!(second.map(|found| found.id), Some(wanted));
    assert_eq!(fx.handle.knowledge_memos().counts_fetched().2, 1);

    fx.handle
        .knowledge_memos()
        .advance_all(NO_PROJECT_LIFETIME + Duration::from_secs(1));
    fx.handle.no_project_knowledge_project().await?;
    assert_eq!(fx.handle.knowledge_memos().counts_fetched().2, 2);
    fx.db.cleanup().await
}

#[tokio::test]
async fn no_project_knowledge_project_that_is_missing_is_not_remembered() -> anyhow::Result<()> {
    let fx = fixture().await?;

    assert!(fx.handle.no_project_knowledge_project().await?.is_none());
    assert!(fx.handle.no_project_knowledge_project().await?.is_none());

    assert_eq!(fx.handle.knowledge_memos().counts_fetched().2, 2);
    fx.db.cleanup().await
}
