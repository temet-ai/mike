use super::knowledge::sweep;
use super::knowledge_tests::{breaker, embedding_server, scan, source};
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::{PersistHandle, test_support::acquire_ephemeral_db};
use uuid::Uuid;

async fn artifact_item(handle: &PersistHandle) -> anyhow::Result<Option<(i64, bool)>> {
    Ok(sqlx::query_as(
        "SELECT i.id, i.needs_embedding FROM memory.knowledge_items i WHERE i.kind = 'artifact'",
    )
    .fetch_optional(handle.pool())
    .await?)
}

#[tokio::test]
async fn reconcile_source_without_item_adds_the_item() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = handle
        .knowledge()
        .define_knowledge_project("Reconcile adds", Uuid::new_v4())
        .await?;
    let location = source(&handle, &project.project_uuid.to_string()).await?;
    scan(&handle, location, 0, "Evidence a watch trigger missed").await?;
    sqlx::query("DELETE FROM memory.knowledge_items WHERE kind = 'artifact'")
        .execute(handle.pool())
        .await?;

    let changed = handle.knowledge().reconcile_knowledge_items().await?;

    assert_eq!(changed, 1);
    assert!(matches!(artifact_item(&handle).await?, Some((_, true))));
    assert_eq!(handle.knowledge().reconcile_knowledge_items().await?, 0);
    db.cleanup().await
}

#[tokio::test]
async fn reconcile_embedded_item_whose_project_moved_requeues_it() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = handle
        .knowledge()
        .define_knowledge_project("Reconcile moves", Uuid::new_v4())
        .await?;
    let other = handle
        .knowledge()
        .define_knowledge_project("Reconcile elsewhere", Uuid::new_v4())
        .await?;
    let location = source(&handle, &project.project_uuid.to_string()).await?;
    scan(&handle, location, 0, "Evidence embedded under one project").await?;
    let (_server, service) = embedding_server().await;
    sweep(&handle, &service, &mut breaker()).await?;
    let Some((item_id, false)) = artifact_item(&handle).await? else {
        anyhow::bail!("the sweep did not embed the artifact item");
    };
    sqlx::query("UPDATE memory.knowledge_embedding_state SET project_id = $1 WHERE item_id = $2")
        .bind(other.id)
        .bind(item_id)
        .execute(handle.pool())
        .await?;

    let changed = handle.knowledge().reconcile_knowledge_items().await?;

    assert_eq!(changed, 1);
    assert_eq!(artifact_item(&handle).await?, Some((item_id, true)));
    db.cleanup().await
}
