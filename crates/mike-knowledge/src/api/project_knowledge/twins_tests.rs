use serde_json::json;
use uuid::Uuid;

use super::attach_twins;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::project_knowledge::KnowledgeQuery;
use crate::store::persistence::test_support::{acquire_ephemeral_db, seed_project};

const REMOTE: &str = "github.com/temet-ai/traceroot";

async fn locate(handle: &PersistHandle, project: i64, root: &str) -> anyhow::Result<()> {
    let location = handle
        .knowledge()
        .add_checkout_location(Uuid::new_v4(), "host", root, project)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the location is free"))?;
    handle
        .knowledge()
        .set_location_origin(location, REMOTE)
        .await?;
    Ok(())
}

async fn insert_lesson(handle: &PersistHandle, project: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id, topic)
         SELECT $1, lk.id, 'a lesson the twin holds', ls.id, lsrc.id, 'twins'
         FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
         WHERE lk.code = 'correction' AND ls.code = 'daily' AND lsrc.code = 'operator'",
    )
    .bind(project)
    .execute(handle.pool())
    .await?;
    Ok(())
}

#[tokio::test]
async fn an_empty_search_names_the_twin_that_shares_its_origin() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let asked = seed_project(handle.pool(), "/mnt/data/projects/temet/traceroot").await?;
    locate(&handle, asked, "/mnt/data/projects/temet/traceroot").await?;
    let twin = seed_project(handle.pool(), "/Users/dawid/projects/temet/traceroot").await?;
    locate(&handle, twin, "/Users/dawid/projects/temet/traceroot").await?;
    insert_lesson(&handle, twin).await?;
    let twin_uuid: Uuid =
        sqlx::query_scalar("SELECT project_uuid FROM projects.projects WHERE id = $1")
            .bind(twin)
            .fetch_one(handle.pool())
            .await?;

    let mut answer = json!({"items": [], "has_more": false});
    attach_twins(&handle, asked, &KnowledgeQuery::default(), &mut answer).await?;

    assert_eq!(answer["twins"].as_array().map(Vec::len), Some(1));
    assert_eq!(answer["twins"][0]["project_uuid"], json!(twin_uuid));
    assert_eq!(
        answer["twins"][0]["project_path"],
        "/Users/dawid/projects/temet/traceroot"
    );
    assert_eq!(answer["twins"][0]["items"], 1);

    db.cleanup().await
}

#[tokio::test]
async fn an_empty_search_names_a_twin_with_the_same_folder_name() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let asked = seed_project(handle.pool(), "/mnt/data/projects/temet/ledger").await?;
    let twin = seed_project(handle.pool(), "/Users/dawid/projects/temet/Ledger").await?;
    let other = seed_project(handle.pool(), "/Users/dawid/projects/temet/unrelated").await?;
    insert_lesson(&handle, twin).await?;
    insert_lesson(&handle, other).await?;

    let mut answer = json!({"items": []});
    attach_twins(&handle, asked, &KnowledgeQuery::default(), &mut answer).await?;

    assert_eq!(answer["twins"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        answer["twins"][0]["project_path"],
        "/Users/dawid/projects/temet/Ledger"
    );

    db.cleanup().await
}

#[tokio::test]
async fn a_search_with_items_or_a_later_page_names_no_twin() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let asked = seed_project(handle.pool(), "/mnt/data/projects/temet/ledger").await?;
    let twin = seed_project(handle.pool(), "/Users/dawid/projects/temet/ledger").await?;
    insert_lesson(&handle, twin).await?;

    let mut found = json!({"items": [{"reference": "lesson:1"}]});
    attach_twins(&handle, asked, &KnowledgeQuery::default(), &mut found).await?;
    let mut later = json!({"items": []});
    let paged = KnowledgeQuery {
        offset: Some(20),
        ..KnowledgeQuery::default()
    };
    attach_twins(&handle, asked, &paged, &mut later).await?;

    assert!(found.get("twins").is_none());
    assert!(later.get("twins").is_none());

    db.cleanup().await
}
