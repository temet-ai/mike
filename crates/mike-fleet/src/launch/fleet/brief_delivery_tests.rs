use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::test_support::seed_fleet_running;
use std::time::Duration;

use serde_json::{Value, json};
use uuid::Uuid;

use super::brief_delivery::{LEAD, TaskPart, compose, first_message, is_delivered};
use super::brief_notes::run_tags;
use super::brief_notes_sync::{StoredPart, ensure_run_part, record_task_part};
use super::brief_version::{GivenPart, facts_for, parts_in, parts_line, task_part_version_in};
use super::definition::{FleetDefinition, merge};
use super::tests::{complete, parsed};
use crate::launch::brief::{
    BriefMode, BriefStaging, DEFAULT_BRIEF_INLINE_MAX, FileFallback, demote_to_file, stage,
};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::NewFleet;
use crate::store::persistence::test_support::acquire_ephemeral_db;

const SEAT: &str = "db-dev-P1";

fn definition(as_notes: bool) -> FleetDefinition {
    let mut value = complete();
    merge(
        &mut value,
        &json!({
            "briefs": {"as_notes": as_notes},
            "coordinator": {"session": Uuid::from_u128(1).to_string()},
            "acceptance": {"session": Uuid::from_u128(2).to_string()},
            "observer": {"session": Uuid::from_u128(3).to_string()},
            "tasks": {"P1": {"seat": SEAT, "session": Uuid::from_u128(11).to_string()}}
        }),
    );
    parsed(&value)
}

fn run_part(note: u128, version: i64) -> StoredPart {
    StoredPart {
        note: Uuid::from_u128(note),
        version,
        body: "The run's part: goal, criteria, rules.".into(),
    }
}

fn task(note: Option<u128>, version: i64) -> TaskPart {
    TaskPart {
        note: note.map(Uuid::from_u128),
        version,
        body: "Your task is P1: schema.\n\nAdd the proposals table.".into(),
    }
}

#[test]
fn the_first_message_carries_both_notes_headed_with_their_ids_and_versions() {
    let text = compose(
        (SEAT, "judge-at-work", "P1"),
        &run_part(21, 3),
        &task(Some(22), 2),
    );
    assert!(is_delivered(&text));
    assert!(text.starts_with(LEAD), "{text}");
    for wanted in [
        "You are seat db-dev-P1 of fleet judge-at-work, a worker.",
        &format!(
            "== Run part: mike session note {}, version 3 ==",
            Uuid::from_u128(21)
        ),
        &format!(
            "== Task part: mike session note {}, version 2 ==",
            Uuid::from_u128(22)
        ),
        "The run's part: goal, criteria, rules.",
        "Add the proposals table.",
    ] {
        assert!(text.contains(wanted), "{wanted} missing from {text}");
    }
    println!("{text}");
}

#[test]
fn a_task_note_not_yet_written_is_named_by_its_tags_and_given_version_one() {
    let text = compose(
        (SEAT, "judge-at-work", "P1"),
        &run_part(21, 1),
        &task(None, 1),
    );
    assert!(
        text.contains("== Task part: mike session note tagged task:P1 and part:task in your own notebook, version 1 =="),
        "{text}"
    );
}

#[test]
fn the_versions_given_are_recorded_in_the_brief_and_read_back() {
    let text = compose(
        (SEAT, "judge-at-work", "P1"),
        &run_part(21, 3),
        &task(Some(22), 2),
    );
    let given = parts_in(&text);
    assert_eq!(
        given,
        vec![
            GivenPart {
                part: "run".into(),
                note: Some(Uuid::from_u128(21)),
                version: 3
            },
            GivenPart {
                part: "task".into(),
                note: Some(Uuid::from_u128(22)),
                version: 2
            },
        ]
    );
    assert_eq!(task_part_version_in(&text), Some(2));
    assert_eq!(task_part_version_in("an older brief"), None);
    assert!(parts_in("Brief parts given: nonsense").is_empty());
    let line = parts_line(&given);
    assert_eq!(parts_in(&line), given);
}

fn staging(dir: &std::path::Path, by_mail: bool) -> BriefStaging<'_> {
    BriefStaging {
        briefs_dir: dir,
        inline_max: DEFAULT_BRIEF_INLINE_MAX,
        notes: true,
        by_mail,
        who: Some("db-dev-P1"),
    }
}

fn long_message() -> String {
    compose(
        (SEAT, "judge-at-work", "P1"),
        &run_part(21, 1),
        &task(None, 1),
    ) + &"x".repeat(900)
}

#[test]
fn a_delivered_first_message_pushed_by_mail_is_a_note_brief_and_writes_no_file()
-> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let briefs = dir.path().join("briefs");
    let session = Uuid::new_v4();
    let staged = stage(
        &long_message(),
        Some(session),
        session,
        &staging(&briefs, true),
    )?;
    assert_eq!(staged.mode, BriefMode::Note);
    assert_eq!(staged.path, None);
    assert!(!briefs.exists());
    Ok(())
}

#[test]
fn a_delivered_first_message_with_the_push_off_is_typed_as_a_note_pointer_and_writes_no_file()
-> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let briefs = dir.path().join("briefs");
    let session = Uuid::new_v4();
    let staged = stage(
        &long_message(),
        Some(session),
        session,
        &staging(&briefs, false),
    )?;
    assert_eq!(staged.mode, BriefMode::Inline);
    assert_eq!(staged.path, None);
    assert!(!briefs.exists());
    let note = staged
        .note
        .ok_or_else(|| anyhow::anyhow!("no note minted"))?;
    assert!(
        staged.delivered.contains(&note.to_string()),
        "{}",
        staged.delivered
    );
    assert!(staged.delivered.contains("list_session_notes"));
    assert!(staged.delivered.is_ascii());
    Ok(())
}

#[test]
fn a_delivered_first_message_with_no_note_to_read_falls_back_to_the_file() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let staged = stage(
        &long_message(),
        None,
        Uuid::new_v4(),
        &staging(dir.path(), false),
    )?;
    assert_eq!(staged.mode, BriefMode::File);
    assert!(staged.path.is_some());
    Ok(())
}

#[test]
fn a_brief_that_is_not_a_delivered_message_is_staged_as_a_file_as_before() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let session = Uuid::new_v4();
    let staged = stage(
        &"y".repeat(900),
        Some(session),
        session,
        &staging(dir.path(), false),
    )?;
    assert_eq!(staged.mode, BriefMode::File);
    assert!(staged.path.is_some());
    Ok(())
}

#[test]
fn a_typed_pointer_whose_launch_row_was_not_written_is_demoted_to_the_file() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let session = Uuid::new_v4();
    let prompt = long_message();
    let staged = stage(&prompt, Some(session), session, &staging(dir.path(), false))?;
    let fallback = FileFallback {
        file_id: session,
        briefs_dir: dir.path(),
        who: Some("db-dev-P1"),
    };
    let demoted = demote_to_file(&staged, &prompt, &fallback)?;
    assert_eq!(demoted.mode, BriefMode::File);
    let path = demoted.path.ok_or_else(|| anyhow::anyhow!("no file"))?;
    assert_eq!(std::fs::read_to_string(path)?, prompt);
    Ok(())
}

async fn booted(
    as_notes: bool,
    run: &str,
) -> anyhow::Result<
    Option<(
        crate::store::persistence::test_support::TestDbHandle,
        PersistHandle,
        crate::store::persistence::fleet::FleetRow,
    )>,
> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(None);
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    for n in [1u128, 2, 3, 11] {
        persist
            .launch()
            .ensure_session_exists(&Uuid::from_u128(n).to_string())
            .await?;
    }
    persist
        .launch()
        .upsert_run(&crate::store::persistence::launch::NewRun {
            run_id: run.to_string(),
            project_path: Some("/srv/project".into()),
            backend: "none".into(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    let definition: Value = serde_json::to_value(definition(as_notes))?;
    let fleet = persist
        .fleet()
        .insert_fleet(&NewFleet {
            slug: "delivery".into(),
            project_path: "/srv/project".into(),
            definition,
            missing: json!([]),
            created_by: None,
        })
        .await?;
    let fleet = seed_fleet_running(&persist, fleet.fleet_uuid, run).await?;
    Ok(Some((db, persist, fleet)))
}

#[tokio::test]
async fn with_notes_on_the_first_message_has_the_run_note_and_the_task_note() -> anyhow::Result<()>
{
    let Some((db, persist, row)) = booted(true, "delivery-on-1").await? else {
        return Ok(());
    };
    let def = FleetDefinition::from_value(&row.definition).map_err(anyhow::Error::msg)?;
    let facts = facts_for(&row, &def).ok_or_else(|| anyhow::anyhow!("no facts"))?;
    let run_note = ensure_run_part(&persist, &def, &facts, Duration::ZERO).await;
    let task_note = record_task_part(&persist, (&def, &facts), ("P1", Uuid::from_u128(11))).await;
    assert!(run_note.is_some() && task_note.is_some());

    let text = first_message(&persist, "delivery-on-1", SEAT)
        .await
        .ok_or_else(|| anyhow::anyhow!("no first message"))?;

    let run_note = run_note.map(|n| n.to_string()).unwrap_or_default();
    let task_note = task_note.map(|n| n.to_string()).unwrap_or_default();
    assert!(
        text.contains(&format!("mike session note {run_note}, version 1")),
        "{text}"
    );
    assert!(
        text.contains(&format!("mike session note {task_note}, version 1")),
        "{text}"
    );
    assert!(text.contains("Your task is P1"), "{text}");
    assert_eq!(task_part_version_in(&text), Some(1));
    assert_eq!(run_tags(&facts.fleet).len(), 2);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn before_the_task_note_exists_the_text_comes_from_the_definition_at_version_one()
-> anyhow::Result<()> {
    let Some((db, persist, row)) = booted(true, "delivery-early-1").await? else {
        return Ok(());
    };
    let def = FleetDefinition::from_value(&row.definition).map_err(anyhow::Error::msg)?;
    let facts = facts_for(&row, &def).ok_or_else(|| anyhow::anyhow!("no facts"))?;
    ensure_run_part(&persist, &def, &facts, Duration::ZERO).await;
    let text = first_message(&persist, "delivery-early-1", SEAT)
        .await
        .ok_or_else(|| anyhow::anyhow!("no first message"))?;
    assert!(
        text.contains("tagged task:P1 and part:task in your own notebook, version 1"),
        "{text}"
    );
    assert!(text.contains("Your task is P1"), "{text}");
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn with_notes_off_there_is_no_first_message_from_notes() -> anyhow::Result<()> {
    let Some((db, persist, _)) = booted(false, "delivery-off-1").await? else {
        return Ok(());
    };
    assert_eq!(first_message(&persist, "delivery-off-1", SEAT).await, None);
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn a_run_note_that_cannot_be_read_leaves_the_file_to_be_written() -> anyhow::Result<()> {
    let Some((db, persist, _)) = booted(true, "delivery-missing-1").await? else {
        return Ok(());
    };
    assert_eq!(
        first_message(&persist, "delivery-missing-1", SEAT).await,
        None
    );
    assert_eq!(
        first_message(&persist, "delivery-missing-1", "no-such-seat").await,
        None
    );
    db.cleanup().await?;
    Ok(())
}
