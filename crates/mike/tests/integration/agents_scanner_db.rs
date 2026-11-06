use crate::test_db::TestDb;

use mike::agents::scanner::scan_dirs;
use mike::store::persistence::PersistHandle;

async fn write_fixture_agent(
    dir: &std::path::Path,
    file_stem: &str,
    name: &str,
    persona_sentence: &str,
) -> anyhow::Result<()> {
    let content = format!(
        "---\nname: {name}\ndescription: fixture agent for scanner tests.\n---\n\n{persona_sentence}\n\nBody text.\n"
    );
    tokio::fs::write(dir.join(format!("{file_stem}.md")), content).await?;
    Ok(())
}

#[tokio::test]
async fn scan_dirs_seeds_persona_alias_for_fixture_agent() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let tmp = tempfile::tempdir()?;
    write_fixture_agent(
        tmp.path(),
        "scanner-fixture-a",
        "scanner-fixture-agent-a",
        "You are a Scanner Fixture Persona Alpha -- exercised only by tests.",
    )
    .await?;

    let inserted = scan_dirs(&handle, &[tmp.path().to_path_buf()]).await?;
    assert_eq!(inserted, 1, "the fixture file must land as one new row");

    let aliases = handle.fetch_alias_map().await?;
    let seeded = aliases
        .iter()
        .find(|a| a.alias == "Scanner Fixture Persona Alpha")
        .ok_or_else(|| anyhow::anyhow!("expected persona alias not seeded"))?;
    assert_eq!(seeded.kind, "persona");

    let agent = handle
        .fetch_agent_by_name("scanner-fixture-agent-a")
        .await?
        .ok_or_else(|| anyhow::anyhow!("fixture agent row missing"))?;
    assert_eq!(seeded.agent_id, agent.id);

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn scan_dirs_rerun_on_unchanged_files_does_not_bump_updated_at() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let tmp = tempfile::tempdir()?;
    write_fixture_agent(
        tmp.path(),
        "scanner-fixture-b",
        "scanner-fixture-agent-b",
        "You are a Scanner Fixture Persona Bravo -- stable across rescans.",
    )
    .await?;

    scan_dirs(&handle, &[tmp.path().to_path_buf()]).await?;
    let first = handle
        .fetch_agent_by_name("scanner-fixture-agent-b")
        .await?
        .ok_or_else(|| anyhow::anyhow!("fixture agent row missing after first scan"))?;

    let second_pass_affected = scan_dirs(&handle, &[tmp.path().to_path_buf()]).await?;
    assert_eq!(
        second_pass_affected, 0,
        "re-scanning an unchanged file must be a true no-op"
    );
    let second = handle
        .fetch_agent_by_name("scanner-fixture-agent-b")
        .await?
        .ok_or_else(|| anyhow::anyhow!("fixture agent row missing after second scan"))?;
    assert_eq!(
        first.updated_at, second.updated_at,
        "unchanged re-scan must not bump updated_at"
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn scan_dirs_persona_collision_does_not_flip_ownership() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    let shared_persona =
        "You are a Shared Fixture Persona -- deliberately duplicated across two agents.";
    let tmp = tempfile::tempdir()?;
    write_fixture_agent(
        tmp.path(),
        "scanner-fixture-c1",
        "scanner-fixture-agent-c1",
        shared_persona,
    )
    .await?;
    write_fixture_agent(
        tmp.path(),
        "scanner-fixture-c2",
        "scanner-fixture-agent-c2",
        shared_persona,
    )
    .await?;

    scan_dirs(&handle, &[tmp.path().to_path_buf()]).await?;

    let aliases = handle.fetch_alias_map().await?;
    let matches: Vec<_> = aliases
        .iter()
        .filter(|a| a.alias == "Shared Fixture Persona")
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "a shared persona must resolve to exactly one agent"
    );

    let first_agent = handle
        .fetch_agent_by_name("scanner-fixture-agent-c1")
        .await?
        .ok_or_else(|| anyhow::anyhow!("c1 agent row missing"))?;
    assert_eq!(
        matches[0].agent_id, first_agent.id,
        "first-scanned agent must keep ownership of the collided persona"
    );

    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn scan_dirs_skips_persona_matching_existing_agent_name() -> anyhow::Result<()> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(());
    };
    let db = factory.acquire_isolated().await?;
    let handle = PersistHandle::connect(&db.database_url).await?;

    handle
        .bulk_upsert_file_backed_agents(
            &["Scanner Fixture Persona Name Clash".to_string()],
            &["/dev/null".to_string()],
            &[None],
        )
        .await?;

    let tmp = tempfile::tempdir()?;
    write_fixture_agent(
        tmp.path(),
        "scanner-fixture-d",
        "scanner-fixture-agent-d",
        "You are a Scanner Fixture Persona Name Clash -- matches a real agent name.",
    )
    .await?;

    scan_dirs(&handle, &[tmp.path().to_path_buf()]).await?;

    let aliases = handle.fetch_alias_map().await?;
    assert!(
        !aliases
            .iter()
            .any(|a| a.alias == "Scanner Fixture Persona Name Clash"),
        "a persona matching an existing agent name must never be seeded as an alias"
    );

    db.cleanup().await?;
    Ok(())
}
