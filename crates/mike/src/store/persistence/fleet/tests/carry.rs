use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::lessons_repo::LessonsPersist as _;
use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use super::seed::{seed_launch, seed_run, seed_session};
use super::{Fixture, fixture, new_fleet};
use crate::launch::fleet::carry::{after_note, warm_lines};
use crate::store::persistence::fleet::CarryOutcome;
use crate::store::persistence::session_notes::{NewSessionNote, SessionNoteUpdate};
use crate::store::persistence::test_support::seed_project;

const TITLE: &str = "Ask the reference seat first";
const TAG: &str = "carry:ask-the-reference-seat-first";

struct World {
    f: Fixture,
    project_id: i64,
    fleet: Uuid,
    author: Uuid,
}

async fn world() -> anyhow::Result<Option<World>> {
    let Some(f) = fixture().await? else {
        return Ok(None);
    };
    let pool = f.handle.pool();
    let project_id = seed_project(pool, "/tmp/fleet-a").await?;
    let fleet = f
        .handle
        .fleet()
        .insert_fleet(&new_fleet("carry-fleet", "/tmp/fleet-a"))
        .await?
        .fleet_uuid;
    let author = Uuid::new_v4();
    seed_session(pool, project_id, author).await?;
    Ok(Some(World {
        f,
        project_id,
        fleet,
        author,
    }))
}

async fn write_note(
    w: &World,
    session: Uuid,
    title: &str,
    tags: &[String],
    data: &Value,
) -> anyhow::Result<Uuid> {
    let note = NewSessionNote {
        kind: Some("finding"),
        title,
        body: Some("the body stays in the note"),
        status: None,
        tags: Some(tags),
        data: Some(data),
    };
    Ok(w.f
        .handle
        .launch()
        .create_session_note(session, note)
        .await?
        .note_uuid)
}

fn carry_data(discipline: &str) -> Value {
    json!({"carry": true, "discipline": discipline, "line": "Ask the reference seat before you search the tree."})
}

async fn counts(w: &World) -> anyhow::Result<Vec<(i32, i32)>> {
    let rows = sqlx::query_as::<_, (i32, i32)>(
        "SELECT runs_given, later_findings FROM memory.carried_lessons ORDER BY lesson_id",
    )
    .fetch_all(w.f.handle.pool())
    .await?;
    Ok(rows)
}

#[tokio::test]
async fn a_note_marked_carry_becomes_one_lesson_candidate_for_the_project_and_discipline()
-> anyhow::Result<()> {
    let Some(w) = world().await? else {
        return Ok(());
    };
    write_note(&w, w.author, TITLE, &[], &carry_data("db-dev")).await?;

    let queue = w.f.handle.lessons().list_lesson_candidates(10).await?;
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].source, "carry");
    assert_eq!(queue[0].stage, "raw");
    assert_eq!(queue[0].project_id, Some(w.project_id));
    assert_eq!(
        queue[0].text,
        "Ask the reference seat before you search the tree."
    );
    assert_eq!(queue[0].carry_runs_given, Some(0));
    assert_eq!(queue[0].carry_later_findings, Some(0));
    let row =
        sqlx::query_as::<_, (String, String)>("SELECT discipline, tag FROM memory.carried_lessons")
            .fetch_one(w.f.handle.pool())
            .await?;
    assert_eq!((row.0.as_str(), row.1.as_str()), ("db-dev", TAG));
    w.f.db.cleanup().await
}

#[tokio::test]
async fn saving_the_same_note_again_neither_adds_a_candidate_nor_moves_a_count()
-> anyhow::Result<()> {
    let Some(w) = world().await? else {
        return Ok(());
    };
    let note = write_note(&w, w.author, TITLE, &[], &carry_data("db-dev")).await?;
    let again = SessionNoteUpdate {
        body: Some("edited body"),
        ..SessionNoteUpdate::default()
    };
    w.f.handle.launch().update_session_note(note, again).await?;
    let outcome = after_note(&w.f.handle, note, &[], &carry_data("db-dev")).await;

    assert_eq!(outcome, CarryOutcome::AlreadyCandidate);
    assert_eq!(
        w.f.handle.lessons().list_lesson_candidates(10).await?.len(),
        1
    );
    assert_eq!(counts(&w).await?, vec![(0, 0)]);
    w.f.db.cleanup().await
}

#[tokio::test]
async fn a_later_finding_with_the_tag_moves_the_finding_count_once_however_often_it_is_saved()
-> anyhow::Result<()> {
    let Some(w) = world().await? else {
        return Ok(());
    };
    write_note(&w, w.author, TITLE, &[], &carry_data("db-dev")).await?;
    let other = Uuid::new_v4();
    seed_session(w.f.handle.pool(), w.project_id, other).await?;
    let tags = vec![TAG.to_string()];
    let finding = write_note(&w, other, "It came back", &tags, &json!({})).await?;
    write_note(
        &w,
        other,
        "Unrelated",
        &["carry:something-else".to_string()],
        &json!({}),
    )
    .await?;
    let touch = SessionNoteUpdate {
        body: Some("more"),
        ..SessionNoteUpdate::default()
    };
    w.f.handle
        .launch()
        .update_session_note(finding, touch)
        .await?;

    assert_eq!(counts(&w).await?, vec![(0, 1)]);
    let queue = w.f.handle.lessons().list_lesson_candidates(10).await?;
    assert_eq!(queue[0].carry_later_findings, Some(1));
    w.f.db.cleanup().await
}

#[tokio::test]
async fn a_second_carry_note_with_the_same_title_counts_against_the_first_candidate()
-> anyhow::Result<()> {
    let Some(w) = world().await? else {
        return Ok(());
    };
    write_note(&w, w.author, TITLE, &[], &carry_data("db-dev")).await?;
    let other = Uuid::new_v4();
    seed_session(w.f.handle.pool(), w.project_id, other).await?;
    write_note(&w, other, TITLE, &[], &carry_data("db-dev")).await?;
    write_note(&w, other, TITLE, &[], &carry_data("rust-developer")).await?;

    assert_eq!(
        w.f.handle.lessons().list_lesson_candidates(10).await?.len(),
        2
    );
    assert_eq!(counts(&w).await?, vec![(0, 1), (0, 0)]);
    w.f.db.cleanup().await
}

#[tokio::test]
async fn an_approved_item_is_read_at_warm_up_and_counted_once_per_run() -> anyhow::Result<()> {
    let Some(w) = world().await? else {
        return Ok(());
    };
    let fleet = w.fleet.to_string();
    write_note(&w, w.author, TITLE, &[], &carry_data("db-dev")).await?;
    let lesson = w.f.handle.lessons().list_lesson_candidates(10).await?[0].id;

    assert!(
        warm_lines(&w.f.handle, &fleet, "run-1", &["db-dev"])
            .await
            .is_empty()
    );
    assert_eq!(counts(&w).await?, vec![(0, 0)]);

    w.f.handle
        .lessons()
        .approve_lesson_candidate(lesson, None, "operator")
        .await?;
    let first = warm_lines(&w.f.handle, &fleet, "run-1", &["db-dev", "db-dev-agent"]).await;
    assert_eq!(
        first,
        vec!["Ask the reference seat before you search the tree.".to_string()]
    );
    warm_lines(&w.f.handle, &fleet, "run-1", &["db-dev"]).await;
    assert_eq!(counts(&w).await?, vec![(1, 0)]);
    warm_lines(&w.f.handle, &fleet, "run-2", &["db-dev"]).await;
    assert_eq!(counts(&w).await?, vec![(2, 0)]);
    assert!(
        warm_lines(&w.f.handle, &fleet, "run-3", &["frontend-developer"])
            .await
            .is_empty()
    );
    assert_eq!(counts(&w).await?, vec![(2, 0)]);
    w.f.db.cleanup().await
}

#[tokio::test]
async fn a_rejected_item_is_not_read_at_warm_up() -> anyhow::Result<()> {
    let Some(w) = world().await? else {
        return Ok(());
    };
    write_note(&w, w.author, TITLE, &[], &carry_data("db-dev")).await?;
    let lesson = w.f.handle.lessons().list_lesson_candidates(10).await?[0].id;
    w.f.handle
        .lessons()
        .reject_lesson_candidate(lesson, "other", None)
        .await?;

    let lines = warm_lines(&w.f.handle, &w.fleet.to_string(), "run-1", &["db-dev"]).await;
    assert!(lines.is_empty());
    assert_eq!(counts(&w).await?, vec![(0, 0)]);
    w.f.db.cleanup().await
}

#[tokio::test]
async fn a_carry_note_names_its_discipline_from_the_authors_launch_when_the_note_does_not()
-> anyhow::Result<()> {
    let Some(w) = world().await? else {
        return Ok(());
    };
    let run = format!("fleet-run-{}", Uuid::new_v4());
    seed_run(&w.f.handle, &run).await?;
    seed_launch(&w.f.handle, &run, "db-dev-T7", Some(w.author), true).await?;
    write_note(&w, w.author, TITLE, &[], &json!({"carry": true})).await?;

    let discipline =
        sqlx::query_scalar::<_, String>("SELECT discipline FROM memory.carried_lessons")
            .fetch_one(w.f.handle.pool())
            .await?;
    assert_eq!(discipline, "db-dev-T7-agent");
    w.f.db.cleanup().await
}

#[tokio::test]
async fn a_carry_note_with_no_discipline_to_name_writes_nothing() -> anyhow::Result<()> {
    let Some(w) = world().await? else {
        return Ok(());
    };
    let note = write_note(&w, w.author, TITLE, &[], &json!({"carry": true})).await?;
    let outcome = after_note(&w.f.handle, note, &[], &json!({"carry": true})).await;

    assert_eq!(outcome, CarryOutcome::Skipped);
    assert!(
        w.f.handle
            .lessons()
            .list_lesson_candidates(10)
            .await?
            .is_empty()
    );
    assert_eq!(counts(&w).await?, Vec::<(i32, i32)>::new());
    w.f.db.cleanup().await
}

#[tokio::test]
async fn a_note_that_does_not_exist_and_a_fleet_id_that_is_not_one_are_skipped()
-> anyhow::Result<()> {
    let Some(w) = world().await? else {
        return Ok(());
    };
    let outcome = after_note(&w.f.handle, Uuid::new_v4(), &[], &json!({"carry": true})).await;
    assert_eq!(outcome, CarryOutcome::Skipped);
    assert!(
        warm_lines(&w.f.handle, "not-a-uuid", "run-1", &["db-dev"])
            .await
            .is_empty()
    );
    assert!(Utc::now().timestamp() > 0);
    w.f.db.cleanup().await
}

#[tokio::test]
async fn a_candidate_from_a_note_is_never_picked_up_by_the_lesson_consolidation()
-> anyhow::Result<()> {
    let Some(w) = world().await? else {
        return Ok(());
    };
    write_note(&w, w.author, TITLE, &[], &carry_data("db-dev")).await?;

    let claimed =
        w.f.handle
            .lessons()
            .claim_raw_lessons_for_consolidation(w.project_id, 10)
            .await?;
    assert!(claimed.is_empty());
    assert!(
        w.f.handle
            .lessons()
            .discover_idle_projects(0)
            .await?
            .is_empty()
    );
    w.f.db.cleanup().await
}
