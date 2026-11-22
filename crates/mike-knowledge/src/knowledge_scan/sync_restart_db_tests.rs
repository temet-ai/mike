use super::scan_test_support::{register, write};
use super::sync::{ScanMemo, scan_machine};
use crate::store::persistence::{
    PersistHandle,
    test_support::{acquire_ephemeral_db, seed_project},
};
use uuid::Uuid;

async fn generation(handle: &PersistHandle, name: &str) -> anyhow::Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT generation FROM memory.knowledge_sources WHERE name = $1")
            .bind(name)
            .fetch_one(handle.pool())
            .await?,
    )
}

#[tokio::test]
async fn a_restarted_scan_leaves_an_unchanged_source_alone() -> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    seed_project(handle.pool(), "/test/scan-restart").await?;
    let home = tempfile::tempdir()?;
    let checkout = tempfile::tempdir()?;
    let machine = Uuid::new_v4();
    register(&handle, "/test/scan-restart", machine, checkout.path()).await?;
    write(
        &checkout.path().join(".claude/skills/alpha/SKILL.md"),
        "---\nname: alpha\ndescription: alpha steps\n---\nDo alpha.",
    )?;

    let first = scan_machine(&handle, machine, home.path(), &mut ScanMemo::default()).await?;
    assert!(first.updated > 0);
    let before = generation(&handle, "skills").await?;

    let restarted = scan_machine(&handle, machine, home.path(), &mut ScanMemo::default()).await?;
    assert_eq!(restarted.updated, 0, "nothing changed on disk");
    assert_eq!(generation(&handle, "skills").await?, before);

    write(
        &checkout.path().join(".claude/skills/alpha/SKILL.md"),
        "---\nname: alpha\ndescription: alpha steps\n---\nDo alpha twice.",
    )?;
    let changed = scan_machine(&handle, machine, home.path(), &mut ScanMemo::default()).await?;
    assert_eq!(changed.updated, 1, "a changed file is still ingested");
    assert_eq!(generation(&handle, "skills").await?, before + 1);
    db.cleanup().await
}
