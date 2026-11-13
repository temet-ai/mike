use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use serde_json::Value;
use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db, seed_project};

const WANTED: &str = "551ce144-0000-4000-8000-000000000001";
const TWIN_ONE: &str = "deadbeef-0000-4000-8000-000000000001";
const TWIN_TWO: &str = "deadbeef-0000-4000-8000-000000000002";
const ELSEWHERE: &str = "99aa0000-0000-4000-8000-000000000001";

struct Fixture {
    db: TestDbHandle,
    handle: PersistHandle,
    project: String,
}

async fn lesson(handle: &PersistHandle, uuid: &str, project: i64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO memory.lessons (uuid, project_id, kind_id, text)
         VALUES ($1, $2, 1, 'lesson body for ' || $1::text)",
    )
    .bind(uuid.parse::<Uuid>()?)
    .bind(project)
    .execute(handle.pool())
    .await?;
    Ok(())
}

async fn fixture() -> anyhow::Result<Fixture> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = "/r/reference/home";
    let home = seed_project(handle.pool(), project).await?;
    let other = seed_project(handle.pool(), "/r/reference/other").await?;
    for uuid in [WANTED, TWIN_ONE, TWIN_TWO] {
        lesson(&handle, uuid, home).await?;
    }
    lesson(&handle, ELSEWHERE, other).await?;
    Ok(Fixture {
        db,
        handle,
        project: project.to_owned(),
    })
}

async fn read(fx: &Fixture, reference: &str) -> anyhow::Result<Value> {
    Ok(fx
        .handle
        .knowledge()
        .read_project_knowledge(Some(&fx.project), reference)
        .await?)
}

async fn read_error(fx: &Fixture, reference: &str) -> anyhow::Result<String> {
    match fx
        .handle
        .knowledge()
        .read_project_knowledge(Some(&fx.project), reference)
        .await
    {
        Ok(found) => anyhow::bail!("`{reference}` was read: {found}"),
        Err(error) => Ok(error.to_string()),
    }
}

#[tokio::test]
async fn read_project_knowledge_bare_uuid_reads_the_item_and_names_the_full_reference()
-> anyhow::Result<()> {
    let fx = fixture().await?;

    let item = read(&fx, WANTED).await?;

    assert_eq!(item["reference_corrected"], format!("lesson:{WANTED}"));
    assert_eq!(item["body"], format!("lesson body for {WANTED}"));
    fx.db.cleanup().await
}

#[tokio::test]
async fn read_project_knowledge_eight_character_prefix_reads_the_unique_item() -> anyhow::Result<()>
{
    let fx = fixture().await?;

    let item = read(&fx, "551ce144").await?;

    assert_eq!(item["reference_corrected"], format!("lesson:{WANTED}"));
    fx.db.cleanup().await
}

#[tokio::test]
async fn read_project_knowledge_kind_with_a_prefix_reads_the_unique_item() -> anyhow::Result<()> {
    let fx = fixture().await?;

    let item = read(&fx, "lesson:551CE144").await?;

    assert_eq!(item["reference_corrected"], format!("lesson:{WANTED}"));
    fx.db.cleanup().await
}

#[tokio::test]
async fn read_project_knowledge_full_kind_reference_carries_no_correction() -> anyhow::Result<()> {
    let fx = fixture().await?;

    let item = read(&fx, &format!("lesson:{WANTED}")).await?;

    assert!(item.get("reference_corrected").is_none(), "{item}");
    fx.db.cleanup().await
}

#[tokio::test]
async fn read_project_knowledge_ambiguous_prefix_lists_every_candidate() -> anyhow::Result<()> {
    let fx = fixture().await?;

    let message = read_error(&fx, "deadbeef").await?;

    assert!(message.contains("`deadbeef` is ambiguous"), "{message}");
    assert!(message.contains(&format!("lesson:{TWIN_ONE}")), "{message}");
    assert!(message.contains(&format!("lesson:{TWIN_TWO}")), "{message}");
    fx.db.cleanup().await
}

#[tokio::test]
async fn read_project_knowledge_prefix_without_a_match_says_what_a_reference_looks_like()
-> anyhow::Result<()> {
    let fx = fixture().await?;

    let message = read_error(&fx, "abcdef12").await?;

    assert!(
        message.contains("No knowledge item matches `abcdef12`"),
        "{message}"
    );
    assert!(message.contains("note:<uuid>"), "{message}");
    fx.db.cleanup().await
}

#[tokio::test]
async fn read_project_knowledge_prefix_of_another_projects_item_is_not_found_in_this_one()
-> anyhow::Result<()> {
    let fx = fixture().await?;

    let message = read_error(&fx, "99aa0000").await?;

    assert!(message.contains("in this project"), "{message}");
    assert!(
        fx.handle
            .knowledge()
            .read_project_knowledge(None, "99aa0000")
            .await
            .is_ok(),
        "without a project the prefix finds the item"
    );
    fx.db.cleanup().await
}

#[tokio::test]
async fn read_project_knowledge_text_that_is_no_reference_says_what_a_reference_looks_like()
-> anyhow::Result<()> {
    let fx = fixture().await?;

    let message = read_error(&fx, "hello").await?;

    assert!(
        message.contains("Invalid knowledge reference `hello`"),
        "{message}"
    );
    assert!(message.contains("note:<uuid>"), "{message}");
    fx.db.cleanup().await
}

#[tokio::test]
async fn read_project_knowledge_unknown_kind_names_the_kind_and_the_readable_ones()
-> anyhow::Result<()> {
    let fx = fixture().await?;

    let message = read_error(&fx, "widget:12345678").await?;

    assert!(
        message.contains("Unknown knowledge source kind `widget`"),
        "{message}"
    );
    assert!(message.contains("lesson"), "{message}");
    fx.db.cleanup().await
}
