use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::events::{EventPayload, NewFleetEvent, QuestionAnswered, QuestionAsked};
use super::tell_reference::reference;
use crate::store::persistence::fleet::FleetRow;
use crate::store::persistence::questions::{AskedQuestion, NoteRef, QuestionRow, STATE_ANSWERED};
use crate::store::persistence::{PersistError, PersistHandle};

pub const ESCALATE_AFTER_MINUTES: i32 = 5;

pub fn who(name: Option<&str>, session: Uuid) -> String {
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => format!("{name} (session {session})"),
        None => format!("session {session}"),
    }
}

fn about_line(note: Option<&NoteRef>, row: &QuestionRow) -> String {
    match (note, row.about_note) {
        (Some(note), _) => format!(
            "\nIt is about the note \"{}\" ({}).",
            note.title, note.note_uuid
        ),
        (None, Some(id)) => format!("\nIt is about the note {id}."),
        (None, None) => String::new(),
    }
}

pub fn asked_mail(row: &QuestionRow, note: Option<&NoteRef>) -> String {
    let waiting = if row.blocked {
        " and is blocked until you settle it"
    } else {
        ""
    };
    format!(
        "[question {id}] {asker} asks you{waiting}:\n{question}{about}\n\nSettle it with the `answer` tool (question_id {id}, answer) or the `decline` tool (question_id {id}). It stays listed under your open questions in `what_changed` view `standing` until you do.",
        id = row.question_uuid,
        asker = who(row.asker_name.as_deref(), row.asker),
        question = row.question,
        about = about_line(note, row),
    )
}

pub async fn ask_and_mail(
    persist: &PersistHandle,
    new: &AskedQuestion<'_>,
    note: Option<&NoteRef>,
) -> Result<(QuestionRow, bool), PersistError> {
    if let Some(to) = new.to_session
        && let Some(closed) = persist.closed_fleet_seat(to).await?
    {
        return Err(PersistError::DataInvariant(closed.refusal(to)));
    }
    let row = persist.ask_question(new).await?;
    record_asked(persist, &row).await;
    let Some(to) = row.to_session else {
        return Ok((row, false));
    };
    let mailed = match persist
        .insert_message(Uuid::new_v4(), to, Some(row.asker), &asked_mail(&row, note))
        .await
    {
        Ok(inserted) => inserted.is_some(),
        Err(err) => {
            tracing::warn!(error = %err, %to, "questions: could not mail a question's recipient");
            false
        }
    };
    Ok((row, mailed))
}

pub fn asked_event(row: &QuestionRow) -> Option<NewFleetEvent> {
    Some(NewFleetEvent {
        run_id: row.run_id.clone()?,
        task: row.to_task.clone(),
        session_uuid: Some(row.asker),
        payload: EventPayload::QuestionAsked(QuestionAsked {
            question_id: row.question_uuid,
        }),
    })
}

pub fn answered_event(row: &QuestionRow) -> Option<NewFleetEvent> {
    if row.state != STATE_ANSWERED {
        return None;
    }
    Some(NewFleetEvent {
        run_id: row.run_id.clone()?,
        task: row.to_task.clone(),
        session_uuid: row.settled_by,
        payload: EventPayload::QuestionAnswered(QuestionAnswered {
            question_id: row.question_uuid,
        }),
    })
}

pub async fn record_asked(persist: &PersistHandle, row: &QuestionRow) {
    append(persist, asked_event(row), "question_asked").await;
}

pub async fn record_answered(persist: &PersistHandle, row: &QuestionRow) {
    append(persist, answered_event(row), "question_answered").await;
}

async fn append(persist: &PersistHandle, event: Option<NewFleetEvent>, what: &str) {
    let Some(event) = event else {
        return;
    };
    if let Err(err) = persist.append_fleet_event(&event).await {
        tracing::warn!(error = %err, what, "fleet: could not record a question event");
    }
}

fn asked_of(row: &QuestionRow) -> String {
    match (&row.to_task, row.to_session) {
        (Some(task), Some(session)) => format!("task {task}, session {session}"),
        (Some(task), None) => format!("task {task}"),
        (None, Some(session)) => format!("session {session}"),
        (None, None) => "nobody".to_owned(),
    }
}

pub fn waited_minutes(row: &QuestionRow, now: DateTime<Utc>) -> i64 {
    (now - row.created_at).num_minutes().max(0)
}

pub fn escalation_mail(row: &QuestionRow, now: DateTime<Utc>) -> String {
    format!(
        "[mike question] {asker} has been blocked for {minutes} minutes on a question to {to} that nobody has settled (question {id}):\n{question}\n\nOrdinary mail does not settle this recorded question. Answer it with the `answer` tool (question_id {id}); if you cannot, `decline` it so the asker asks someone else.",
        asker = who(row.asker_name.as_deref(), row.asker),
        minutes = waited_minutes(row, now),
        to = asked_of(row),
        id = row.question_uuid,
        question = row.question,
    )
}

pub fn digest_line(row: &QuestionRow, now: DateTime<Utc>) -> String {
    format!(
        "{asker} has waited {minutes} minutes on {to} (question {id}): {question}; settle with `answer` or `decline`, ordinary mail does not close it",
        asker = who(row.asker_name.as_deref(), row.asker),
        minutes = waited_minutes(row, now),
        to = asked_of(row),
        id = row.question_uuid,
        question = row.question,
    )
}

pub async fn blocked_waiting(
    persist: &PersistHandle,
    run: &str,
    now: DateTime<Utc>,
) -> Vec<QuestionRow> {
    blocked_waiting_since(persist, run, (now, ESCALATE_AFTER_MINUTES)).await
}

pub async fn blocked_waiting_since(
    persist: &PersistHandle,
    run: &str,
    (now, minutes): (DateTime<Utc>, i32),
) -> Vec<QuestionRow> {
    match persist.open_questions_in_run(run).await {
        Ok(rows) => rows
            .into_iter()
            .filter(|q| q.blocked && waited_minutes(q, now) >= i64::from(minutes))
            .collect(),
        Err(err) => {
            tracing::warn!(%run, error = %err, "fleet: could not read the run's open questions");
            Vec::new()
        }
    }
}

async fn claim_due(persist: &PersistHandle, run: &str, minutes: i32) -> Vec<QuestionRow> {
    match persist.escalate_due_questions(run, minutes).await {
        Ok(due) => due,
        Err(err) => {
            tracing::warn!(%run, error = %err, "fleet: could not claim the blocked questions due for the reference seat");
            Vec::new()
        }
    }
}

async fn mail_reference(
    persist: &PersistHandle,
    reference: Uuid,
    question: &QuestionRow,
) -> Option<Uuid> {
    let body = escalation_mail(question, Utc::now());
    match persist
        .insert_message(Uuid::new_v4(), reference, None, &body)
        .await
    {
        Ok(_) => Some(question.question_uuid),
        Err(err) => {
            tracing::warn!(error = %err, question = %question.question_uuid, "fleet: could not mail the reference seat a blocked question");
            None
        }
    }
}

pub async fn escalate_run(persist: &PersistHandle, def: &FleetDefinition, run: &str) -> Vec<Uuid> {
    escalate_after(persist, def, (run, ESCALATE_AFTER_MINUTES)).await
}

pub async fn escalate_after(
    persist: &PersistHandle,
    def: &FleetDefinition,
    (run, minutes): (&str, i32),
) -> Vec<Uuid> {
    let Some(reference) = reference(def) else {
        return Vec::new();
    };
    let mut escalated = Vec::new();
    for question in claim_due(persist, run, minutes).await {
        if question.to_session == Some(reference) {
            continue;
        }
        escalated.extend(mail_reference(persist, reference, &question).await);
    }
    escalated
}

pub async fn escalate(persist: &PersistHandle, row: &FleetRow) {
    let (Ok(def), Some(run)) = (
        FleetDefinition::from_value(&row.definition),
        row.run_id.as_deref(),
    ) else {
        return;
    };
    escalate_run(persist, &def, run).await;
}
