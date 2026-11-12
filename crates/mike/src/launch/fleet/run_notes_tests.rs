use std::path::Path;

use serde_json::json;
use uuid::Uuid;

use super::bundle;
use super::close_note;
use super::definition::FleetDefinition;
use super::plan_tests::{facts, seats};
use super::run_notebook::own_session;
use super::run_text_notes::record_in;
use super::shutdown;
use super::tests::changed;
use crate::config::ProxyConfig;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetRow;
use crate::store::persistence::fleet::NewFleet;
use crate::store::persistence::launch::NewRun;
use crate::store::persistence::session_notes::{
    NoteOrder, ProjectNoteFilter, SessionNoteFilter, SessionNoteRow,
};
use crate::store::persistence::test_support::{
    TestDbHandle, acquire_ephemeral_db, seed_fleet_running,
};

struct Setup {
    db: TestDbHandle,
    persist: PersistHandle,
    repo: tempfile::TempDir,
    state: tempfile::TempDir,
}

async fn booted() -> anyhow::Result<Option<Setup>> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    Ok(Some(Setup {
        db,
        persist,
        repo: tempfile::tempdir()?,
        state: tempfile::tempdir()?,
    }))
}

fn project_file(repo: &Path, name: &str, text: &str) -> anyhow::Result<()> {
    let dir = repo.join(".claude").join("fleet");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(name), text)?;
    Ok(())
}

fn definition(repo: &Path, db_contract: &str) -> FleetDefinition {
    changed(json!({
        "repo": repo.to_string_lossy(),
        "workers": {"db-dev": {"contract": db_contract}},
    }))
}

async fn notes_with_tag(
    persist: &PersistHandle,
    session: Uuid,
    tag: &str,
) -> anyhow::Result<Vec<SessionNoteRow>> {
    let filter = SessionNoteFilter {
        kind: Some("note"),
        status: None,
        tag: Some(tag),
        q: None,
        order: NoteOrder::OldestFirst,
        limit: 50,
        offset: 0,
    };
    Ok(persist.fetch_session_notes(session, filter).await?)
}

fn only(notes: Vec<SessionNoteRow>) -> anyhow::Result<SessionNoteRow> {
    anyhow::ensure!(notes.len() == 1, "expected one note, found {notes:?}");
    Ok(notes.into_iter().next().unwrap_or_else(|| unreachable!()))
}

#[tokio::test]
async fn the_rules_and_each_contract_a_run_uses_are_recorded_once_with_where_they_came_from()
-> anyhow::Result<()> {
    let Some(setup) = booted().await? else {
        return Ok(());
    };
    let (persist, facts) = (&setup.persist, facts());
    project_file(setup.repo.path(), "rules.md", "PROJECT RULES\nsecond line")?;
    project_file(setup.repo.path(), "worker-contract.md", "WORKER CONTRACT")?;
    project_file(setup.repo.path(), "db-contract.md", "DB CONTRACT")?;
    let def = definition(setup.repo.path(), ".claude/fleet/db-contract.md");
    persist
        .ensure_session_exists(&seats().coordinator.to_string())
        .await?;

    let written = record_in(persist, (&def, &facts), setup.state.path()).await;
    let again = record_in(persist, (&def, &facts), setup.state.path()).await;

    assert_eq!((written, again), (3, 0));
    let rules = only(notes_with_tag(persist, seats().coordinator, "rules").await?)?;
    let path = setup.repo.path().join(".claude/fleet/rules.md");
    assert_eq!(
        rules.body,
        format!(
            "Source: project file {}\n\nPROJECT RULES\nsecond line",
            path.display()
        )
    );
    assert_eq!(
        rules.tags,
        vec![
            format!("fleet:{}", facts.fleet),
            facts.run_id.clone(),
            "rules".to_string()
        ]
    );
    for (file, text) in [
        ("db-contract.md", "DB CONTRACT"),
        ("worker-contract.md", "WORKER CONTRACT"),
    ] {
        let tag = format!("contract:{file}");
        let note = only(notes_with_tag(persist, seats().coordinator, &tag).await?)?;
        assert!(
            note.body.starts_with("Source: project file "),
            "{}",
            note.body
        );
        assert!(note.body.ends_with(&format!("\n\n{text}")), "{}", note.body);
        assert!(note.tags.contains(&facts.run_id));
    }
    setup.db.cleanup().await
}

#[tokio::test]
async fn text_mike_supplies_is_recorded_as_built_into_mike() -> anyhow::Result<()> {
    let Some(setup) = booted().await? else {
        return Ok(());
    };
    let (persist, facts) = (&setup.persist, facts());
    bundle::install_files(setup.state.path()).map_err(anyhow::Error::msg)?;
    let def = definition(setup.repo.path(), ".claude/fleet/worker-contract.md");
    persist
        .ensure_session_exists(&seats().coordinator.to_string())
        .await?;

    let written = record_in(persist, (&def, &facts), setup.state.path()).await;

    assert_eq!(written, 2);
    let rules = only(notes_with_tag(persist, seats().coordinator, "rules").await?)?;
    let built_in = bundle::generic_file("rules.md").unwrap_or_default();
    assert_eq!(rules.body, format!("Source: built into mike\n\n{built_in}"));
    let contract =
        only(notes_with_tag(persist, seats().coordinator, "contract:worker-contract.md").await?)?;
    assert!(contract.body.starts_with("Source: built into mike\n\n"));
    setup.db.cleanup().await
}

#[tokio::test]
async fn with_no_coordinator_session_the_notes_go_to_mikes_own_session_under_the_project()
-> anyhow::Result<()> {
    let Some(setup) = booted().await? else {
        return Ok(());
    };
    let (persist, facts) = (&setup.persist, facts());
    project_file(setup.repo.path(), "rules.md", "PROJECT RULES")?;
    let repo = setup.repo.path().to_string_lossy().into_owned();
    let def = definition(setup.repo.path(), "");

    record_in(persist, (&def, &facts), setup.state.path()).await;

    let own = own_session(&facts.run_id);
    let rules = only(notes_with_tag(persist, own, "rules").await?)?;
    assert!(rules.body.contains("PROJECT RULES"));
    let project = persist
        .find_project_id_by_path(&repo)
        .await?
        .ok_or_else(|| anyhow::anyhow!("the project was not found"))?;
    let filter = ProjectNoteFilter {
        role: None,
        name: None,
        kind: Some("note"),
        status: None,
        tag: Some(&facts.run_id),
        session_uuid: None,
        q: None,
        order: NoteOrder::OldestFirst,
        limit: 50,
        offset: 0,
    };
    let (total, found) = persist.fetch_project_notes(project, filter).await?;
    assert_eq!(total, 1, "{found:?}");
    setup.db.cleanup().await
}

async fn running_fleet(
    persist: &PersistHandle,
    repo: &Path,
    run: &str,
) -> anyhow::Result<(FleetRow, FleetDefinition)> {
    let def = definition(repo, "");
    let fleet = persist
        .insert_fleet(&NewFleet {
            slug: "close-note".into(),
            project_path: repo.to_string_lossy().into_owned(),
            definition: serde_json::to_value(&def)?,
            missing: json!([]),
            created_by: None,
        })
        .await?;
    persist
        .upsert_run(&NewRun {
            run_id: run.to_string(),
            project_path: Some(repo.to_string_lossy().into_owned()),
            backend: "none".into(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let running = seed_fleet_running(persist, fleet.fleet_uuid, run).await?;
    Ok((running, def))
}

#[tokio::test]
async fn stopping_a_fleet_leaves_one_close_note_with_its_state_time_and_tasks() -> anyhow::Result<()>
{
    let Some(setup) = booted().await? else {
        return Ok(());
    };
    let persist = &setup.persist;
    let run = "fleet-close-note-2";
    let (running, _) = running_fleet(persist, setup.repo.path(), run).await?;
    let config = ProxyConfig::try_parse_from([
        "mike",
        "--upstream-url",
        "https://api.anthropic.com",
        "--proxy-listen-addr",
        "127.0.0.1:0",
        "--inspect-listen-addr",
        "127.0.0.1:0",
        "--persist-enabled",
        "1",
        "--database-url",
        &setup.db.database_url,
    ])?;
    let state = AppState::new(config).await?;

    let report = shutdown::stop(&state, persist, &running).await?;
    shutdown::stop(&state, persist, &running).await?;

    assert_eq!(report.state, "done");
    let note = only(notes_with_tag(persist, own_session(run), "close").await?)?;
    for wanted in [
        "is done.",
        "Stop requested by: mike fleet stop",
        "Acceptance: not every task was accepted.",
        "Tasks done (0): none",
        "Tasks open (3): P1 (queued), P2 (queued), P10 (queued)",
    ] {
        assert!(
            note.body.contains(wanted),
            "{wanted} missing from {}",
            note.body
        );
    }
    assert_eq!(note.data["stop_requested_by"], json!("mike fleet stop"));
    state.shutdown().cancel();
    setup.db.cleanup().await
}

#[tokio::test]
async fn a_close_note_without_a_status_says_the_tasks_were_not_read() -> anyhow::Result<()> {
    let Some(setup) = booted().await? else {
        return Ok(());
    };
    let persist = &setup.persist;
    let run = "fleet-close-note-1";
    let (row, _) = running_fleet(persist, setup.repo.path(), run).await?;

    close_note::record(persist, &row, None).await;

    let note = only(notes_with_tag(persist, own_session(run), "close").await?)?;
    assert!(note.tags.contains(&format!("fleet:{}", row.fleet_uuid)));
    assert!(note.tags.contains(&run.to_string()));
    assert!(note.body.contains("Tasks: not read"), "{}", note.body);
    assert!(
        note.body.contains("Closed at: not recorded"),
        "{}",
        note.body
    );
    setup.db.cleanup().await
}
