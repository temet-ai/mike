use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::test_support::{acquire_ephemeral_db, seed_project};

async fn location(
    handle: &PersistHandle,
    project: i64,
    machine: Uuid,
    root: &str,
) -> anyhow::Result<Uuid> {
    Ok(sqlx::query_scalar(
        "INSERT INTO projects.locations(project_id, machine_uuid, root_path)
         VALUES ($1, $2, $3) RETURNING location_uuid",
    )
    .bind(project)
    .bind(machine)
    .bind(root)
    .fetch_one(handle.pool())
    .await?)
}

async fn source(
    handle: &PersistHandle,
    location: Uuid,
    name: &str,
    root: &str,
) -> anyhow::Result<Uuid> {
    Ok(sqlx::query_scalar(
        "INSERT INTO memory.knowledge_sources(location_uuid, name, root_path)
         VALUES ($1, $2, $3) RETURNING source_uuid",
    )
    .bind(location)
    .bind(name)
    .bind(root)
    .fetch_one(handle.pool())
    .await?)
}

async fn root_of(handle: &PersistHandle, source: Uuid) -> anyhow::Result<String> {
    Ok(
        sqlx::query_scalar("SELECT root_path FROM memory.knowledge_sources WHERE source_uuid = $1")
            .bind(source)
            .fetch_one(handle.pool())
            .await?,
    )
}

#[tokio::test]
async fn finish_location_moves_rewrites_checkout_and_memory_roots_and_leaves_outside_sources()
-> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let project = seed_project(handle.pool(), "/test/source-roots").await?;
    let machine = Uuid::new_v4();
    handle.ensure_machine(machine, "host").await?;
    let main = location(&handle, project, machine, "/work/old").await?;
    let tree = location(&handle, project, machine, "/work/old/.claude/worktrees/wt").await?;
    let skills = source(&handle, main, "skills", "/work/old/.claude/skills").await?;
    let memory = source(
        &handle,
        main,
        "claude-memory",
        "/home/u/.claude/projects/-work-old/memory",
    )
    .await?;
    let tree_memory = source(
        &handle,
        tree,
        "claude-memory",
        "/home/u/.claude/projects/-work-old--claude-worktrees-wt/memory",
    )
    .await?;
    let outside = source(&handle, main, "notes", "/elsewhere/notes").await?;

    let mut moves = vec![handle.move_location(main, "/work/new").await?];
    handle.knowledge().finish_location_moves(&mut moves).await?;

    assert_eq!(moves[0].sources_rewritten, 3);
    assert_eq!(root_of(&handle, skills).await?, "/work/new/.claude/skills");
    assert_eq!(
        root_of(&handle, memory).await?,
        "/home/u/.claude/projects/-work-new/memory"
    );
    assert_eq!(
        root_of(&handle, tree_memory).await?,
        "/home/u/.claude/projects/-work-new--claude-worktrees-wt/memory"
    );
    assert_eq!(root_of(&handle, outside).await?, "/elsewhere/notes");
    db.cleanup().await
}
