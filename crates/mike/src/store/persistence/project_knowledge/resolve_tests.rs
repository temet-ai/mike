use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support::{TestDbHandle, acquire_ephemeral_db};

struct Fixture {
    db: TestDbHandle,
    handle: PersistHandle,
}

struct Seeded {
    id: i64,
    uuid: Uuid,
}

async fn fixture() -> anyhow::Result<Fixture> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    Ok(Fixture { db, handle })
}

async fn seed(
    handle: &PersistHandle,
    path: &str,
    display: &str,
    custom: Option<&str>,
) -> anyhow::Result<Seeded> {
    let (id, uuid): (i64, Uuid) = sqlx::query_as(
        "INSERT INTO projects.projects (project_path, display_name, custom_name)
         VALUES ($1, $2, $3) RETURNING id, project_uuid",
    )
    .bind(path)
    .bind(display)
    .bind(custom)
    .fetch_one(handle.pool())
    .await?;
    Ok(Seeded { id, uuid })
}

async fn resolved_id(handle: &PersistHandle, selector: &str) -> anyhow::Result<i64> {
    Ok(handle.resolve_knowledge_project(selector).await?.id)
}

async fn resolve_message(handle: &PersistHandle, selector: &str) -> anyhow::Result<String> {
    match handle.resolve_knowledge_project(selector).await {
        Ok(found) => anyhow::bail!("`{selector}` resolved to project {}", found.id),
        Err(error) => Ok(error.to_string()),
    }
}

#[tokio::test]
async fn resolve_knowledge_project_by_uuid_returns_that_project() -> anyhow::Result<()> {
    let fx = fixture().await?;
    let project = seed(&fx.handle, "/r/by-uuid/alpha", "alpha", None).await?;

    assert_eq!(
        resolved_id(&fx.handle, &project.uuid.to_string()).await?,
        project.id
    );
    fx.db.cleanup().await
}

#[tokio::test]
async fn resolve_knowledge_project_by_numeric_id_returns_that_project() -> anyhow::Result<()> {
    let fx = fixture().await?;
    let project = seed(&fx.handle, "/r/by-id/alpha", "alpha", None).await?;

    assert_eq!(
        resolved_id(&fx.handle, &project.id.to_string()).await?,
        project.id
    );
    fx.db.cleanup().await
}

#[tokio::test]
async fn resolve_knowledge_project_by_path_returns_that_project() -> anyhow::Result<()> {
    let fx = fixture().await?;
    let project = seed(&fx.handle, "/r/by-path/alpha", "alpha", None).await?;

    assert_eq!(
        resolved_id(&fx.handle, "/r/by-path/alpha").await?,
        project.id
    );
    fx.db.cleanup().await
}

#[tokio::test]
async fn resolve_knowledge_project_by_location_root_returns_the_owning_project()
-> anyhow::Result<()> {
    let fx = fixture().await?;
    let project = seed(&fx.handle, "/r/by-root/home", "home", None).await?;
    fx.handle
        .add_checkout_location(Uuid::new_v4(), "host", "/r/by-root/checkout", project.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the location is free"))?;

    assert_eq!(
        resolved_id(&fx.handle, "/r/by-root/checkout").await?,
        project.id
    );
    fx.db.cleanup().await
}

#[tokio::test]
async fn resolve_knowledge_project_by_name_ignores_case_and_reads_both_name_columns()
-> anyhow::Result<()> {
    let fx = fixture().await?;
    let custom = seed(&fx.handle, "/r/by-name/one", "Shown One", Some("Traceroot")).await?;
    let display = seed(&fx.handle, "/r/by-name/two", "Gamma Ray", None).await?;

    assert_eq!(resolved_id(&fx.handle, "traceroot").await?, custom.id);
    assert_eq!(resolved_id(&fx.handle, "GAMMA RAY").await?, display.id);
    fx.db.cleanup().await
}

#[tokio::test]
async fn resolve_knowledge_project_by_last_path_component_ignores_case() -> anyhow::Result<()> {
    let fx = fixture().await?;
    let project = seed(&fx.handle, "/r/by-leaf/Wombat", "Something Else", None).await?;

    assert_eq!(resolved_id(&fx.handle, "wombat").await?, project.id);
    fx.db.cleanup().await
}

#[tokio::test]
async fn resolve_knowledge_project_name_match_beats_last_path_component() -> anyhow::Result<()> {
    let fx = fixture().await?;
    let named = seed(&fx.handle, "/r/level/gg", "radar", None).await?;
    seed(&fx.handle, "/r/level/radar", "/r/level/radar", None).await?;

    assert_eq!(resolved_id(&fx.handle, "radar").await?, named.id);
    fx.db.cleanup().await
}

#[tokio::test]
async fn resolve_knowledge_project_skips_a_merged_project_when_matching_by_name()
-> anyhow::Result<()> {
    let fx = fixture().await?;
    let live = seed(&fx.handle, "/r/merged/live", "dup-name", None).await?;
    let old = seed(&fx.handle, "/r/merged/old", "dup-name", None).await?;
    sqlx::query("UPDATE projects.projects SET merged_into_id = $1 WHERE id = $2")
        .bind(live.id)
        .bind(old.id)
        .execute(fx.handle.pool())
        .await?;

    assert_eq!(resolved_id(&fx.handle, "dup-name").await?, live.id);
    fx.db.cleanup().await
}

#[tokio::test]
async fn resolve_knowledge_project_shared_path_names_every_match_with_its_uuid()
-> anyhow::Result<()> {
    let fx = fixture().await?;
    let owner = seed(&fx.handle, "/r/shared/radar", "radar", None).await?;
    let other = seed(&fx.handle, "/r/shared/gg", "gg", None).await?;
    fx.handle
        .add_checkout_location(Uuid::new_v4(), "host", "/r/shared/radar", other.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the location is free"))?;

    let message = resolve_message(&fx.handle, "/r/shared/radar").await?;

    assert!(
        message.contains("`/r/shared/radar` is ambiguous"),
        "{message}"
    );
    assert!(message.contains(&owner.uuid.to_string()), "{message}");
    assert!(message.contains(&other.uuid.to_string()), "{message}");
    assert!(message.contains("/r/shared/gg"), "{message}");
    assert!(
        message.contains("Pass one of these project UUIDs"),
        "{message}"
    );
    assert_eq!(
        resolved_id(&fx.handle, &other.uuid.to_string()).await?,
        other.id
    );
    fx.db.cleanup().await
}

#[tokio::test]
async fn resolve_knowledge_project_unknown_selector_lists_projects_that_contain_it()
-> anyhow::Result<()> {
    let fx = fixture().await?;
    let near = seed(&fx.handle, "/r/unknown/zanzibar-service", "zanzibar", None).await?;

    let message = resolve_message(&fx.handle, "zanzib").await?;

    assert!(message.contains("Unknown project `zanzib`"), "{message}");
    assert!(message.contains(&near.uuid.to_string()), "{message}");
    assert!(message.contains("/r/unknown/zanzibar-service"), "{message}");
    fx.db.cleanup().await
}

#[tokio::test]
async fn resolve_knowledge_project_unknown_selector_without_near_matches_says_none_contain_it()
-> anyhow::Result<()> {
    let fx = fixture().await?;

    let message = resolve_message(&fx.handle, "no-such-project-anywhere").await?;

    assert!(
        message.contains("no project's name or path contains it"),
        "{message}"
    );
    fx.db.cleanup().await
}

#[tokio::test]
async fn find_knowledge_project_unknown_selector_is_none_and_ambiguous_selector_is_an_error()
-> anyhow::Result<()> {
    let fx = fixture().await?;
    seed(&fx.handle, "/r/find/a", "twin", None).await?;
    seed(&fx.handle, "/r/find/b", "twin", None).await?;

    assert!(
        fx.handle
            .find_knowledge_project("/r/find/missing")
            .await?
            .is_none()
    );
    assert!(fx.handle.find_knowledge_project("twin").await.is_err());
    fx.db.cleanup().await
}
