use super::scan_test_support::{live_documents, register, write};
use super::sync::{ScanMemo, scan_machine};
use crate::store::persistence::{
    PersistHandle,
    test_support::{acquire_ephemeral_db, seed_project},
};
use uuid::Uuid;
#[tokio::test]
async fn scan_machine_indexes_the_claude_home_sources_and_the_checkout_instructions()
-> anyhow::Result<()> {
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    seed_project(handle.pool(), "/test/scan-machine-sources").await?;
    let home = tempfile::tempdir()?;
    let repo = tempfile::tempdir()?;
    let machine = Uuid::new_v4();
    register(&handle, "/test/scan-machine-sources", machine, repo.path()).await?;
    write(
        &home.path().join("agents/dev.md"),
        "---\nname: dev\n---\nDevelops.",
    )?;
    write(&home.path().join("rules/style.md"), "# Style")?;
    write(&home.path().join("CLAUDE.md"), "# Global instructions")?;
    write(
        &home.path().join("skills/alpha/SKILL.md"),
        "---\nname: alpha\n---\nSteps",
    )?;
    write(
        &home.path().join("agent-memory/dev/note.md"),
        "---\nname: note\n---\nRemember.",
    )?;
    write(&repo.path().join("CLAUDE.md"), "# Checkout instructions")?;
    write(
        &repo.path().join(".claude/rules/mike-lessons.md"),
        "<!-- mike-learning-generated:v1 -->\n# L",
    )?;
    write(&repo.path().join(".claude/rules/kept.md"), "# Kept")?;
    write(&repo.path().join("README.md"), "# Not an instruction")?;

    let mut memo = ScanMemo::default();
    let first = scan_machine(&handle, machine, home.path(), &mut memo).await?;
    assert_eq!((first.scanned, first.updated, first.failed), (5, 5, 0));
    assert_eq!(
        live_documents(&handle, "agents").await?,
        vec!["dev.md".to_owned()]
    );
    assert_eq!(
        live_documents(&handle, "rules").await?,
        vec!["CLAUDE.md".to_owned(), "style.md".to_owned()]
    );
    assert_eq!(
        live_documents(&handle, "skills").await?,
        vec!["alpha/SKILL.md".to_owned()]
    );
    assert_eq!(
        live_documents(&handle, "agent-memory").await?,
        vec!["dev/note.md".to_owned()]
    );
    assert_eq!(
        live_documents(&handle, "instructions").await?,
        vec![".claude/rules/kept.md".to_owned(), "CLAUDE.md".to_owned()]
    );

    let second = scan_machine(&handle, machine, home.path(), &mut memo).await?;
    assert_eq!((second.scanned, second.updated, second.failed), (5, 0, 0));
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_machine_with_no_checkout_registers_its_sources_once_it_is_recorded() -> anyhow::Result<()>
{
    let db = acquire_ephemeral_db()
        .await?
        .ok_or_else(|| anyhow::anyhow!("MIKE_TEST_ADMIN_URL required"))?;
    let handle = PersistHandle::connect(&db.database_url).await?;
    let home = tempfile::tempdir()?;
    write(
        &home.path().join("agents/dev.md"),
        "---\nname: dev\n---\nDevelops.",
    )?;
    let machine = Uuid::new_v4();
    let mut memo = ScanMemo::default();

    let unrecorded = scan_machine(&handle, machine, home.path(), &mut memo).await?;
    assert_eq!(unrecorded.updated, 0);
    assert_eq!(unrecorded.failed, 1);

    handle
        .ensure_machine(machine, "pi-without-checkouts")
        .await?;
    let recorded = scan_machine(&handle, machine, home.path(), &mut memo).await?;
    assert_eq!((recorded.updated, recorded.failed), (1, 0));
    assert_eq!(
        live_documents(&handle, "agents").await?,
        vec!["dev.md".to_owned()]
    );
    db.cleanup().await
}
