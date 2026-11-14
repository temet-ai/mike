use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::time::Duration;

use serde_json::{Value, json};
use uuid::Uuid;

use super::brief_notes::{
    as_notes, change_mail, lines_that_differ, run_part, run_tags, task_part, task_tags, worker_file,
};
use super::brief_notes_sync::{ensure_run_part, latest_part, record_task_part, run_note_id, sync};
use super::definition::FleetDefinition;
use super::plan::{template, worker};
use super::plan_briefs::worker_prompt;
use super::plan_tests::{acceptance, coordinator, facts};
use super::tests::{changed, complete, parsed};
use crate::change_record::types::{ChangeKind, ChangeQuery};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::session_notes::{NoteOrder, SessionNoteFilter, SessionNoteRow};
use crate::store::persistence::test_support::acquire_ephemeral_db;

fn on(patch: Value) -> FleetDefinition {
    let mut value = complete();
    super::definition::merge(&mut value, &json!({"briefs": {"as_notes": true}}));
    super::definition::merge(&mut value, &patch);
    parsed(&value)
}

fn seated(patch: Value) -> FleetDefinition {
    let mut seated = json!({"tasks": {"P1": {
        "seat": "db-dev-P1", "session": Uuid::from_u128(11).to_string()}}});
    super::definition::merge(&mut seated, &patch);
    on(seated)
}

#[test]
fn a_definition_keeps_briefs_off_unless_it_asks_for_notes() -> anyhow::Result<()> {
    assert!(!as_notes(&parsed(&complete())));
    assert!(!as_notes(&changed(json!({"briefs": {}}))));
    let asked = on(json!({}));
    assert!(as_notes(&asked));
    let stored = serde_json::to_value(&asked)?;
    assert_eq!(stored["briefs"]["as_notes"], json!(true));
    Ok(())
}

#[test]
fn the_runs_part_holds_the_goal_ask_criteria_rules_checks_and_shared_files() {
    let def = on(json!({"check": {"command": "make check", "paths": ["crates/"]}}));
    let part = run_part(&def, &facts());
    for wanted in [
        "Goal: The improvement pages show what the judge did",
        "make the judge's work visible",
        "C1: proposals are stored",
        "C2: the page lists proposals",
        "no seat works on its own branch",
        "make check",
        "crates/mike/src/store/persistence/mod.rs",
    ] {
        assert!(part.contains(wanted), "{wanted} is missing from {part}");
    }
    for control in [acceptance(), coordinator()] {
        assert!(
            !part.contains(&control.to_string()),
            "a fleet opens no control seat, yet {control} is in {part}"
        );
    }
    assert!(!part.contains("Add the proposals table."), "{part}");
}

#[test]
fn a_tasks_part_holds_its_name_brief_criteria_ids_and_files_and_nothing_of_the_run() {
    let def = on(json!({}));
    let part = task_part(&def, "P1").unwrap_or_default();
    for wanted in [
        "Your task is P1: schema.",
        "Add the proposals table.",
        "Criteria this task serves: C1",
        "crates/mike/src/store/persistence/improvement/",
    ] {
        assert!(part.contains(wanted), "{wanted} is missing from {part}");
    }
    assert!(!part.contains("proposals are stored"), "{part}");
    assert!(task_part(&def, "P99").is_none());
}

#[test]
fn a_template_is_given_the_runs_part_only_for_a_fleet_that_asks_for_notes() {
    let facts = facts();
    let def = on(json!({}));
    let Some(plan) = template(&def, &facts, "db-dev") else {
        panic!("a template for db-dev");
    };
    assert!(plan.prompt.contains(&run_part(&def, &facts)));
    let off = parsed(&complete());
    let Some(plain) = template(&off, &facts, "db-dev") else {
        panic!("a template for db-dev");
    };
    assert!(!plain.prompt.contains("The run's part of every brief"));
}

#[test]
fn the_file_a_worker_reads_is_its_tasks_part_then_one_pointer_to_the_runs_note() {
    let facts = facts();
    let def = on(json!({}));
    let note = Uuid::from_u128(77);
    let file = worker_file(&def, &facts, ("db-dev-P1", "P1"), Some(note)).unwrap_or_default();
    assert!(file.contains("You are seat db-dev-P1 of fleet judge-at-work, a worker."));
    assert!(file.contains("Add the proposals table."));
    assert!(file.contains(&format!("mike session note {note}")));
    assert!(file.contains("already in your context from the template"));
    assert!(file.contains("list_session_notes"));
    assert!(file.contains("get_fleet"));
    assert!(file.contains("view \"seat\""));
    assert!(!file.contains("Rules particular to this run"), "{file}");
    assert!(!file.contains("proposals are stored"), "{file}");
    let again = worker_file(&def, &facts, ("db-dev-P1", "P1"), None).unwrap_or_default();
    assert!(
        again.contains("the mike session note tagged part:run"),
        "{again}"
    );
    let session = Uuid::from_u128(11);
    let examples = super::tool_examples::brief(&facts.fleet, session, Some("P1"));
    let Some(plan) = worker(&def, &facts, "P1", session) else {
        panic!("worker P1");
    };
    assert_eq!(plan.session, session);
    assert_eq!(plan.prompt, format!("{again}\n\n{examples}"));
}

#[test]
fn with_notes_off_the_worker_brief_is_the_one_every_fleet_gets() {
    let facts = facts();
    let off = parsed(&complete());
    let Some(task) = off.tasks.get("P1") else {
        panic!("task P1");
    };
    let expected = worker_prompt(&off, &facts, ("db-dev-P1", "P1"), task);
    let session = Uuid::from_u128(11);
    let examples = super::tool_examples::brief(&facts.fleet, session, Some("P1"));
    let Some(plan) = worker(&off, &facts, "P1", session) else {
        panic!("worker P1");
    };
    assert_eq!(plan.session, session);
    assert_eq!(plan.prompt, format!("{expected}\n\n{examples}"));
    assert!(!expected.contains("part:run"));
}

#[test]
fn only_the_lines_that_differ_are_mailed_with_the_old_and_the_new() {
    let old = "Your task is P1: schema.\n\nAdd the proposals table.\n\nFiles:\n- a/";
    let new = "Your task is P1: schema.\n\nAdd the table and an index.\n\nFiles:\n- a/";
    let differing = lines_that_differ(old, new);
    assert_eq!(
        differing,
        "- Add the proposals table.\n+ Add the table and an index."
    );
    let note = Uuid::from_u128(5);
    let mail = change_mail("your task P1", (1, 2), &differing, note);
    assert!(mail.contains("version 1, now 2"));
    assert!(mail.contains(&differing));
    assert!(!mail.contains("Your task is P1: schema."));
    assert!(mail.contains(&note.to_string()));
    assert_eq!(lines_that_differ(old, old), "");
}

async fn notes_of(
    persist: &PersistHandle,
    session: Uuid,
    tag: &str,
) -> anyhow::Result<Vec<SessionNoteRow>> {
    let filter = SessionNoteFilter {
        kind: Some("brief"),
        status: None,
        tag: Some(tag),
        q: None,
        order: NoteOrder::OldestFirst,
        limit: 50,
        offset: 0,
    };
    Ok(persist
        .launch()
        .fetch_session_notes(session, filter)
        .await?)
}

async fn inbox(persist: &PersistHandle, session: Uuid) -> anyhow::Result<Vec<String>> {
    Ok(persist
        .launch()
        .fetch_for_session_with_activity(session)
        .await?
        .inbox
        .into_iter()
        .map(|m| m.row.body)
        .collect())
}

async fn registered_seats(persist: &PersistHandle, sessions: &[Uuid]) -> anyhow::Result<()> {
    for session in sessions {
        persist
            .launch()
            .ensure_session_exists(&session.to_string())
            .await?;
    }
    Ok(())
}

#[tokio::test]
async fn the_runs_part_is_one_note_in_the_coordinators_notebook_written_once() -> anyhow::Result<()>
{
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let facts = facts();
    let def = on(json!({}));
    registered_seats(&persist, &[coordinator()]).await?;
    let first = ensure_run_part(&persist, &def, &facts, Duration::ZERO).await;
    let again = ensure_run_part(&persist, &def, &facts, Duration::ZERO).await;
    assert!(first.is_some());
    assert_eq!(first, again);
    assert_eq!(run_note_id(&persist, &facts).await, first);
    let notes = notes_of(&persist, coordinator(), "part:run").await?;
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].data["part"], json!("run"));
    assert_eq!(notes[0].data["version"], json!(1));
    assert!(notes[0].tags.contains(&format!("fleet:{}", facts.fleet)));
    assert_eq!(notes[0].body, run_part(&def, &facts));
    assert_eq!(
        run_tags(&facts.fleet),
        vec![format!("fleet:{}", facts.fleet), "part:run".to_string()]
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_seat_that_has_not_registered_gets_no_run_note_and_no_error() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let facts = facts();
    let def = on(json!({}));
    assert_eq!(
        ensure_run_part(&persist, &def, &facts, Duration::ZERO).await,
        None
    );
    assert_eq!(run_note_id(&persist, &facts).await, None);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_tasks_part_is_a_note_in_its_seats_notebook_found_again_by_its_tags() -> anyhow::Result<()>
{
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let facts = facts();
    let def = seated(json!({}));
    let seat = Uuid::from_u128(11);
    registered_seats(&persist, &[seat]).await?;
    let written = record_task_part(&persist, (&def, &facts), ("P1", seat)).await;
    let twice = record_task_part(&persist, (&def, &facts), ("P1", seat)).await;
    assert!(written.is_some());
    assert_eq!(written, twice);
    let tags = task_tags(&facts.fleet, "P1");
    let found = latest_part(&persist, seat, &tags).await;
    assert_eq!(found.as_ref().map(|part| part.note), written);
    assert_eq!(found.map(|part| part.version), Some(1));
    let notes = notes_of(&persist, seat, "part:task").await?;
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].data["task"], json!("P1"));
    assert_eq!(notes[0].data["part"], json!("task"));
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_changed_task_is_a_new_version_naming_the_old_note_and_the_seat_gets_only_the_lines_that_differ()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let mut facts = facts();
    facts.run_id = "fleet-notes-versions-1".into();
    let seat = Uuid::from_u128(11);
    registered_seats(&persist, &[coordinator(), seat]).await?;
    let open = [coordinator(), seat];
    let before = seated(json!({}));
    ensure_run_part(&persist, &before, &facts, Duration::ZERO).await;
    let first = record_task_part(&persist, (&before, &facts), ("P1", seat)).await;
    sync(&persist, (&before, &facts), &open).await;
    assert_eq!(notes_of(&persist, seat, "part:task").await?.len(), 1);
    assert!(inbox(&persist, seat).await?.is_empty());

    let after = seated(json!({"tasks": {"P1": {"brief": "Add the table and an index."}}}));
    sync(&persist, (&after, &facts), &open).await;
    sync(&persist, (&after, &facts), &open).await;

    let notes = notes_of(&persist, seat, "part:task").await?;
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[1].data["version"], json!(2));
    assert_eq!(notes[1].data["supersedes"], json!(first));
    assert!(notes[1].body.contains("Add the table and an index."));
    let mail = inbox(&persist, seat).await?;
    assert_eq!(mail.len(), 1, "{mail:?}");
    assert!(mail[0].contains("- Add the proposals table."));
    assert!(mail[0].contains("+ Add the table and an index."));
    assert!(!mail[0].contains("Your task is P1: schema."), "{}", mail[0]);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_changed_rule_is_a_new_run_version_and_every_open_seat_is_mailed_the_differing_lines()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let mut facts = facts();
    facts.run_id = "fleet-notes-versions-2".into();
    let seat = Uuid::from_u128(11);
    registered_seats(&persist, &[coordinator(), seat]).await?;
    let open = [coordinator(), seat];
    let before = seated(json!({}));
    let first = ensure_run_part(&persist, &before, &facts, Duration::ZERO).await;
    let after = seated(json!({"rules": ["no seat works on its own branch", "no deploys"]}));
    sync(&persist, (&after, &facts), &open).await;

    let notes = notes_of(&persist, coordinator(), "part:run").await?;
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[1].data["version"], json!(2));
    assert_eq!(notes[1].data["supersedes"], json!(first));
    for session in open {
        let mail = inbox(&persist, session).await?;
        assert_eq!(mail.len(), 1, "{session}: {mail:?}");
        assert!(mail[0].contains("+ - no deploys"), "{}", mail[0]);
    }
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_brief_note_is_an_event_in_the_change_record_only_while_the_toggle_is_on()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let mut facts = facts();
    facts.run_id = "fleet-notes-events".into();
    let seat = Uuid::from_u128(11);
    registered_seats(&persist, &[coordinator(), seat]).await?;
    persist.upsert_project_returning_id("/srv/project").await?;
    let def = seated(json!({}));
    let everything = ChangeQuery::everything_after("/srv/project", 0, 50);

    ensure_run_part(&persist, &def, &facts, Duration::ZERO).await;
    assert!(
        persist
            .fleet()
            .change_events_after(&everything)
            .await?
            .is_empty()
    );

    persist.upsert_feature_toggle("change_record", true).await?;
    record_task_part(&persist, (&def, &facts), ("P1", seat)).await;
    let events = persist.fleet().change_events_after(&everything).await?;
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].kind, ChangeKind::Brief);
    assert_eq!(events[0].task.as_deref(), Some("P1"));
    assert_eq!(events[0].run_id.as_deref(), Some("fleet-notes-events"));
    assert!(
        events[0]
            .paths
            .contains(&"crates/mike/src/store/persistence/mod.rs".to_string())
    );
    assert_eq!(events[0].detail["version"], json!(1));
    db.cleanup().await?;
    Ok(())
}
