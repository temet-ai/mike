use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::time::Duration;

use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use super::brief_notes::{AMENDMENTS_MARK, amended, carrying_amendments};
use super::brief_notes_sync::{amend_part, ensure_run_part, latest_part, record_task_part, sync};
use super::definition::FleetDefinition;
use super::events::{EventPayload, NewFleetEvent, QuestionAnswered, QuestionAsked};
use super::plan_tests::{coordinator, facts};
use super::questions::{
    answered_event, asked_event, blocked_waiting, blocked_waiting_since, digest_line,
    escalate_after, escalate_run, escalation_mail,
};
use super::tests::{complete, parsed};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::launch::NewRun;
use crate::store::persistence::questions::{AskedQuestion, QuestionRow};
use crate::store::persistence::test_support::acquire_ephemeral_db;

#[test]
fn a_question_asked_in_a_run_records_question_asked_for_its_task() {
    assert_eq!(
        asked_event(&row(false, 0)),
        Some(NewFleetEvent {
            run_id: "run-1".into(),
            task: Some("T3".into()),
            session_uuid: Some(Uuid::from_u128(2)),
            payload: EventPayload::QuestionAsked(QuestionAsked {
                question_id: Uuid::from_u128(7),
            }),
        })
    );
}

#[test]
fn a_question_outside_a_run_records_no_event() {
    let mut asked = row(false, 0);
    asked.run_id = None;
    assert_eq!(asked_event(&asked), None);
}

#[test]
fn an_answered_question_records_question_answered_by_its_settler() {
    let mut settled = row(false, 0);
    settled.state = "answered".into();
    settled.settled_by = Some(Uuid::from_u128(3));
    assert_eq!(
        answered_event(&settled),
        Some(NewFleetEvent {
            run_id: "run-1".into(),
            task: Some("T3".into()),
            session_uuid: Some(Uuid::from_u128(3)),
            payload: EventPayload::QuestionAnswered(QuestionAnswered {
                question_id: Uuid::from_u128(7),
            }),
        })
    );
}

#[test]
fn a_declined_question_records_no_answer_event() {
    let mut settled = row(false, 0);
    settled.state = "declined".into();
    settled.settled_by = Some(Uuid::from_u128(3));
    assert_eq!(answered_event(&settled), None);
}

fn definition(patch: Value) -> FleetDefinition {
    let mut value = complete();
    super::definition::merge(&mut value, &json!({"briefs": {"as_notes": true}}));
    super::definition::merge(&mut value, &patch);
    parsed(&value)
}

fn row(blocked: bool, minutes_ago: i64) -> QuestionRow {
    QuestionRow {
        question_uuid: Uuid::from_u128(7),
        run_id: Some("run-1".into()),
        asker: Uuid::from_u128(2),
        asker_name: Some("db-dev-T5".into()),
        to_session: Some(Uuid::from_u128(3)),
        to_task: Some("T3".into()),
        about_note: None,
        question: "which file holds the resolver?".into(),
        blocked,
        state: "open".into(),
        answer: None,
        settled_by: None,
        created_at: Utc::now() - chrono::Duration::minutes(minutes_ago),
        settled_at: None,
        escalated_at: None,
    }
}

#[test]
fn the_reference_seat_is_told_who_waits_how_long_on_whom_and_how_to_settle_it() {
    let text = escalation_mail(&row(true, 7), Utc::now());
    for wanted in [
        "db-dev-T5 (session 00000000-0000-0000-0000-000000000002)",
        "blocked for 7 minutes",
        "task T3, session 00000000-0000-0000-0000-000000000003",
        "which file holds the resolver?",
        "`answer` tool (question_id 00000000-0000-0000-0000-000000000007)",
        "`decline`",
    ] {
        assert!(text.contains(wanted), "{wanted} is missing from {text}");
    }
    let line = digest_line(&row(true, 9), Utc::now());
    assert!(line.starts_with("db-dev-T5 (session "));
    assert!(line.contains("waited 9 minutes on task T3"));
    assert!(!line.contains('\n'));
}

#[test]
fn an_amendment_is_added_once_under_its_mark_and_carried_into_a_regenerated_brief() {
    let first = amended("Your task is T5.", "Question 1: a?\nAnswer: b");
    assert!(first.ends_with(&format!("\n\n{AMENDMENTS_MARK}\nQuestion 1: a?\nAnswer: b")));
    let second = amended(&first, "Question 2: c?\nAnswer: d");
    assert_eq!(second.matches(AMENDMENTS_MARK).count(), 1);
    assert!(second.ends_with("Question 1: a?\nAnswer: b\n\nQuestion 2: c?\nAnswer: d"));

    let carried = carrying_amendments("Your task is T5, changed.", &second);
    assert!(carried.starts_with("Your task is T5, changed.\n\n"));
    assert!(carried.ends_with("Question 2: c?\nAnswer: d"));
    assert_eq!(
        carrying_amendments("fresh", "an old brief with no answers"),
        "fresh"
    );
}

async fn open_session(persist: &PersistHandle, session: Uuid) -> anyhow::Result<()> {
    persist
        .launch()
        .ensure_session_exists(&session.to_string())
        .await?;
    Ok(())
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

async fn run_row(persist: &PersistHandle, run: &str) -> anyhow::Result<()> {
    persist
        .launch()
        .upsert_run(&NewRun {
            run_id: run.to_owned(),
            project_path: None,
            backend: "herdr".to_owned(),
            workspace_ref: None,
            tab_ref: None,
        })
        .await?;
    Ok(())
}

#[tokio::test]
async fn a_blocked_question_goes_to_the_reference_seat_once_and_is_listed_for_the_digest()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let (asker, recipient, reference) = (
        Uuid::from_u128(21),
        Uuid::from_u128(22),
        Uuid::from_u128(23),
    );
    for session in [asker, recipient, reference] {
        open_session(&persist, session).await?;
    }
    run_row(&persist, "run-esc").await?;
    let def = definition(json!({"reference": {"enabled": true, "session": reference.to_string()}}));
    let base = AskedQuestion {
        asker,
        to_session: Some(recipient),
        to_task: Some("T3"),
        run_id: Some("run-esc"),
        about_note: None,
        question: "which file?",
        blocked: true,
    };
    let blocked = persist.launch().ask_question(&base).await?;
    persist
        .launch()
        .ask_question(&AskedQuestion {
            blocked: false,
            question: "no hurry",
            ..base
        })
        .await?;

    assert!(
        blocked_waiting(&persist, "run-esc", Utc::now())
            .await
            .is_empty()
    );
    assert!(escalate_run(&persist, &def, "run-esc").await.is_empty());
    let waiting = blocked_waiting_since(&persist, "run-esc", (Utc::now(), 0)).await;
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].question_uuid, blocked.question_uuid);

    let first = escalate_after(&persist, &def, ("run-esc", 0)).await;
    assert_eq!(first, vec![blocked.question_uuid]);
    let again = escalate_after(&persist, &def, ("run-esc", 0)).await;
    assert!(again.is_empty());
    let mail = inbox(&persist, reference).await?;
    assert_eq!(mail.len(), 1, "{mail:?}");
    assert!(mail[0].contains(&blocked.question_uuid.to_string()));
    assert!(mail[0].contains("which file?"));
    assert_eq!(
        blocked_waiting_since(&persist, "run-esc", (Utc::now(), 0))
            .await
            .len(),
        1,
        "an escalated question stays open and listed"
    );
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn nothing_is_claimed_when_the_fleet_has_no_reference_seat_and_a_question_to_it_is_not_mailed()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let (asker, reference) = (Uuid::from_u128(31), Uuid::from_u128(33));
    open_session(&persist, asker).await?;
    open_session(&persist, reference).await?;
    run_row(&persist, "run-none").await?;
    let asked = persist
        .launch()
        .ask_question(&AskedQuestion {
            asker,
            to_session: Some(reference),
            to_task: None,
            run_id: Some("run-none"),
            about_note: None,
            question: "to the reference itself",
            blocked: true,
        })
        .await?;

    let off =
        definition(json!({"reference": {"enabled": false, "session": reference.to_string()}}));
    assert!(
        escalate_after(&persist, &off, ("run-none", 0))
            .await
            .is_empty()
    );
    let unclaimed = persist
        .launch()
        .question_by_uuid(asked.question_uuid)
        .await?;
    assert!(unclaimed.is_some_and(|q| q.escalated_at.is_none()));

    let on = definition(json!({"reference": {"enabled": true, "session": reference.to_string()}}));
    assert!(
        escalate_after(&persist, &on, ("run-none", 0))
            .await
            .is_empty()
    );
    assert!(inbox(&persist, reference).await?.is_empty());
    let claimed = persist
        .launch()
        .question_by_uuid(asked.question_uuid)
        .await?;
    assert!(claimed.is_some_and(|q| q.escalated_at.is_some()));
    db.cleanup().await?;
    Ok(())
}

async fn notes(persist: &PersistHandle, session: Uuid) -> anyhow::Result<Vec<(i64, String)>> {
    use crate::store::persistence::session_notes::{NoteOrder, SessionNoteFilter};
    let filter = SessionNoteFilter {
        kind: Some("brief"),
        status: None,
        tag: Some("part:task"),
        q: None,
        order: NoteOrder::OldestFirst,
        limit: 50,
        offset: 0,
    };
    let rows = persist
        .launch()
        .fetch_session_notes(session, filter)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.data["version"].as_i64().unwrap_or(0), r.body))
        .collect())
}

#[tokio::test]
async fn an_answer_that_changes_a_brief_is_a_new_version_that_later_syncs_keep()
-> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let mut facts = facts();
    facts.run_id = "fleet-amend-1".into();
    let seat = Uuid::from_u128(11);
    let open = [coordinator(), seat];
    for session in open {
        open_session(&persist, session).await?;
    }
    let seated = |brief: &str| {
        definition(json!({"tasks": {"P1": {
            "seat": "db-dev-P1", "session": seat.to_string(), "brief": brief}}}))
    };
    let before = seated("Add the proposals table.");
    ensure_run_part(&persist, &before, &facts, Duration::ZERO).await;
    let Some(note) = record_task_part(&persist, (&before, &facts), ("P1", seat)).await else {
        anyhow::bail!("the task part was not recorded");
    };
    let Some(reference) = persist.launch().note_ref(note).await? else {
        anyhow::bail!("the task part has no note");
    };

    let stored = amend_part(&persist, &reference, "Question 1: index?\nAnswer: yes").await;
    let stored = stored.map_err(anyhow::Error::msg)?;
    assert_eq!(stored.version, 2);
    let after_amend = notes(&persist, seat).await?;
    assert_eq!(after_amend.len(), 2);
    assert!(after_amend[1].1.contains(AMENDMENTS_MARK));
    assert!(after_amend[1].1.ends_with("Answer: yes"));
    let title = persist
        .launch()
        .note_ref(stored.note)
        .await?
        .map(|n| n.title);
    assert_eq!(title.as_deref(), Some("Brief, task P1 (version 2)"));

    sync(&persist, (&before, &facts), &open).await;
    assert_eq!(
        notes(&persist, seat).await?.len(),
        2,
        "sync kept the answer"
    );
    assert!(inbox(&persist, seat).await?.is_empty());

    let changed = seated("Add the table and an index.");
    sync(&persist, (&changed, &facts), &open).await;
    let latest = latest_part(
        &persist,
        seat,
        &["part:task".to_owned(), format!("fleet:{}", facts.fleet)],
    )
    .await;
    let Some(latest) = latest else {
        anyhow::bail!("no latest part");
    };
    assert_eq!(latest.version, 3);
    assert!(latest.body.contains("Add the table and an index."));
    assert!(latest.body.ends_with("Answer: yes"));
    assert!(!latest.body.contains("Add the proposals table."));
    let mail = inbox(&persist, seat).await?;
    assert_eq!(mail.len(), 1, "{mail:?}");
    assert!(!mail[0].contains("Answer: yes"));
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn only_a_part_of_a_brief_can_be_amended() -> anyhow::Result<()> {
    let Some(db) = acquire_ephemeral_db().await? else {
        eprintln!("MIKE_TEST_ADMIN_URL unset; skipping");
        return Ok(());
    };
    let persist = PersistHandle::connect(&db.database_url).await?;
    let author = Uuid::from_u128(41);
    open_session(&persist, author).await?;
    let created = persist
        .fleet()
        .create_session_note(
            author,
            crate::store::persistence::session_notes::NewSessionNote {
                kind: Some("finding"),
                title: "A finding",
                body: Some("text"),
                status: None,
                tags: None,
                data: None,
            },
        )
        .await?;
    let Some(reference) = persist.launch().note_ref(created.note_uuid).await? else {
        anyhow::bail!("the note was not stored");
    };
    let refused = amend_part(&persist, &reference, "x").await;
    assert!(refused.is_err_and(|m| m.contains("not a part of a brief")));
    db.cleanup().await?;
    Ok(())
}
