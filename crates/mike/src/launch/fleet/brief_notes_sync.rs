use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::time::Duration;

use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use super::brief_notes::{
    BRIEF_KIND, PART_RUN, PART_TASK, amended, carrying_amendments, change_mail, lines_that_differ,
    run_part, run_tags, task_part, task_tags,
};
use super::definition::FleetDefinition;
use super::plan::FleetFacts;
use crate::change_record::types::{ChangeKind, NewChangeEvent};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::questions::NoteRef;
use crate::store::persistence::session_notes::{
    NewSessionNote, NoteOrder, SessionNoteFilter, SessionNoteRow,
};

const REGISTRATION_WAIT: Duration = Duration::from_secs(60);
const REGISTRATION_POLL: Duration = Duration::from_secs(2);
const NOTES_READ: i64 = 100;
const CHANGE_RECORD_TOGGLE: &str = "change_record";

#[derive(Debug, Clone, PartialEq)]
pub struct StoredPart {
    pub note: Uuid,
    pub version: i64,
    pub body: String,
}

impl StoredPart {
    fn of(row: &SessionNoteRow) -> Self {
        Self {
            note: row.note_uuid,
            version: row.data["version"].as_i64().unwrap_or(1),
            body: row.body.clone(),
        }
    }
}

pub async fn latest_part(
    persist: &PersistHandle,
    session: Uuid,
    tags: &[String],
) -> Option<StoredPart> {
    let first = tags.first()?;
    let filter = SessionNoteFilter {
        kind: Some(BRIEF_KIND),
        status: None,
        tag: Some(first.as_str()),
        q: None,
        order: NoteOrder::NewestFirst,
        limit: NOTES_READ,
        offset: 0,
    };
    let rows = persist
        .launch()
        .fetch_session_notes(session, filter)
        .await
        .ok()?;
    rows.iter()
        .filter(|row| tags.iter().all(|tag| row.tags.contains(tag)))
        .map(StoredPart::of)
        .max_by_key(|part| part.version)
}

pub async fn run_note_id(persist: &PersistHandle, facts: &FleetFacts) -> Option<Uuid> {
    let tags = run_tags(&facts.fleet);
    latest_part(persist, facts.seats.coordinator, &tags)
        .await
        .map(|part| part.note)
}

async fn registered(persist: &PersistHandle, session: Uuid, patience: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + patience;
    loop {
        if let Ok(Some(_)) = persist.resolve_session_db_id(&session.to_string()).await {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(REGISTRATION_POLL).await;
    }
}

struct NewPart<'a> {
    tags: Vec<String>,
    title: String,
    body: &'a str,
    data: Value,
}

async fn store(
    persist: &PersistHandle,
    session: Uuid,
    part: NewPart<'_>,
) -> Result<StoredPart, String> {
    let note = NewSessionNote {
        kind: Some(BRIEF_KIND),
        title: &part.title,
        body: Some(part.body),
        status: None,
        tags: Some(part.tags.as_slice()),
        data: Some(&part.data),
    };
    persist
        .launch()
        .create_session_note(session, note)
        .await
        .map(|row| StoredPart::of(&row))
        .map_err(|e| e.to_string())
}

fn carried(fresh: String, before: Option<&StoredPart>) -> String {
    match before {
        Some(part) => carrying_amendments(&fresh, &part.body),
        None => fresh,
    }
}

pub async fn amend_part(
    persist: &PersistHandle,
    note: &NoteRef,
    entry: &str,
) -> Result<StoredPart, String> {
    let (part, task) = if note.tags.iter().any(|t| t == PART_TASK) {
        let task = note
            .tags
            .iter()
            .find_map(|t| t.strip_prefix("task:"))
            .ok_or_else(|| "the task brief note names no task".to_string())?;
        ("task", Some(task.to_string()))
    } else if note.tags.iter().any(|t| t == PART_RUN) {
        ("run", None)
    } else {
        return Err("the note is not a part of a brief".to_string());
    };
    let tags: Vec<String> = note
        .tags
        .iter()
        .filter(|t| {
            t.starts_with("fleet:") || t.starts_with("task:") || *t == PART_TASK || *t == PART_RUN
        })
        .cloned()
        .collect();
    let latest = latest_part(persist, note.author, &tags)
        .await
        .ok_or_else(|| "the brief has no stored version to amend".to_string())?;
    let version = latest.version + 1;
    let body = amended(&latest.body, entry);
    let title = note.title.replacen(
        &format!("version {}", note.version.unwrap_or(latest.version)),
        &format!("version {version}"),
        1,
    );
    let new = NewPart {
        tags,
        title,
        body: &body,
        data: data_for(part, task.as_deref(), version, Some(latest.note)),
    };
    store(persist, note.author, new).await
}

fn data_for(part: &str, task: Option<&str>, version: i64, before: Option<Uuid>) -> Value {
    let mut data = json!({"part": part, "version": version});
    if let Some(task) = task {
        data["task"] = json!(task);
    }
    if let Some(before) = before {
        data["supersedes"] = json!(before);
    }
    data
}

async fn change_record_on(persist: &PersistHandle) -> bool {
    match persist.fetch_feature_toggles().await {
        Ok(rows) => rows
            .iter()
            .find(|(key, _, _)| key == CHANGE_RECORD_TOGGLE)
            .is_some_and(|row| row.1),
        Err(_) => false,
    }
}

struct BriefEvent<'a> {
    session: Uuid,
    task: Option<&'a str>,
    paths: &'a [String],
    subject: String,
    detail: Value,
}

async fn record_event(
    persist: &PersistHandle,
    def: &FleetDefinition,
    facts: &FleetFacts,
    event: BriefEvent<'_>,
) {
    if !change_record_on(persist).await {
        return;
    }
    let Ok(Some(project_id)) = persist.find_project_id_by_path(&def.repo).await else {
        return;
    };
    let session_id = persist
        .resolve_session_db_id(&event.session.to_string())
        .await
        .ok()
        .flatten();
    let new = NewChangeEvent {
        project_id,
        checkout: def.repo.clone(),
        at: Utc::now(),
        kind: ChangeKind::Brief,
        session_id,
        run_id: Some(facts.run_id.clone()),
        task: event.task.map(str::to_string),
        paths: event.paths.to_vec(),
        subject: event.subject,
        detail: event.detail,
        before_sha: None,
        after_sha: None,
    };
    if let Err(err) = persist.fleet().insert_change_events(&[new]).await {
        tracing::warn!(run = %facts.run_id, error = %err, "mike could not record a brief note in the change record");
    }
}

async fn write_run_part(
    persist: &PersistHandle,
    (def, facts): (&FleetDefinition, &FleetFacts),
    before: Option<&StoredPart>,
) -> Option<StoredPart> {
    let body = carried(run_part(def, facts), before);
    let version = before.map_or(1, |part| part.version + 1);
    let part = NewPart {
        tags: run_tags(&facts.fleet),
        title: format!("Brief, run part (version {version}): fleet {}", facts.slug),
        body: &body,
        data: data_for("run", None, version, before.map(|part| part.note)),
    };
    let stored = match store(persist, facts.seats.coordinator, part).await {
        Ok(stored) => stored,
        Err(problem) => {
            tracing::warn!(run = %facts.run_id, %problem, "mike could not store the run's part of the briefs as a note");
            return None;
        }
    };
    let event = BriefEvent {
        session: facts.seats.coordinator,
        task: None,
        paths: &def.shared_files,
        subject: format!("run part of the briefs, version {version}"),
        detail: json!({"note": stored.note, "part": "run", "version": version}),
    };
    record_event(persist, def, facts, event).await;
    Some(stored)
}

pub async fn ensure_run_part(
    persist: &PersistHandle,
    def: &FleetDefinition,
    facts: &FleetFacts,
    patience: Duration,
) -> Option<Uuid> {
    if !registered(persist, facts.seats.coordinator, patience).await {
        return None;
    }
    let tags = run_tags(&facts.fleet);
    if let Some(found) = latest_part(persist, facts.seats.coordinator, &tags).await {
        return Some(found.note);
    }
    write_run_part(persist, (def, facts), None)
        .await
        .map(|part| part.note)
}

pub async fn record_task_part(
    persist: &PersistHandle,
    (def, facts): (&FleetDefinition, &FleetFacts),
    (task_id, session): (&str, Uuid),
) -> Option<Uuid> {
    if !registered(persist, session, REGISTRATION_WAIT).await {
        return None;
    }
    write_task_part(persist, (def, facts), (task_id, session), None)
        .await
        .map(|part| part.note)
}

async fn write_task_part(
    persist: &PersistHandle,
    (def, facts): (&FleetDefinition, &FleetFacts),
    (task_id, session): (&str, Uuid),
    before: Option<&StoredPart>,
) -> Option<StoredPart> {
    let tags = task_tags(&facts.fleet, task_id);
    if before.is_none()
        && let Some(found) = latest_part(persist, session, &tags).await
    {
        return Some(found);
    }
    let body = carried(task_part(def, task_id)?, before);
    let version = before.map_or(1, |part| part.version + 1);
    let part = NewPart {
        tags,
        title: format!("Brief, task {task_id} (version {version})"),
        body: &body,
        data: data_for("task", Some(task_id), version, before.map(|part| part.note)),
    };
    let stored = match store(persist, session, part).await {
        Ok(stored) => stored,
        Err(problem) => {
            tracing::warn!(run = %facts.run_id, %task_id, %problem, "mike could not store a task's part of the briefs as a note");
            return None;
        }
    };
    let files = def.tasks.get(task_id).map(|t| t.files.clone());
    let event = BriefEvent {
        session,
        task: Some(task_id),
        paths: files.as_deref().unwrap_or_default(),
        subject: format!("task {task_id} brief, version {version}"),
        detail: json!({"note": stored.note, "part": "task", "task": task_id, "version": version}),
    };
    record_event(persist, def, facts, event).await;
    Some(stored)
}

async fn mail_once(
    persist: &PersistHandle,
    facts: &FleetFacts,
    (key, to): (String, Uuid),
    body: &str,
) {
    if super::told::already(persist, &facts.run_id, &key).await {
        return;
    }
    match persist
        .launch()
        .insert_message(Uuid::new_v4(), to, None, body)
        .await
    {
        Ok(_) => super::told::mark(persist, &facts.run_id, &[key]).await,
        Err(err) => {
            tracing::warn!(run = %facts.run_id, %key, error = %err, "mike could not mail a seat the lines of its brief that changed");
        }
    }
}

async fn refresh_run_part(
    persist: &PersistHandle,
    (def, facts): (&FleetDefinition, &FleetFacts),
    seats: &[Uuid],
) {
    let tags = run_tags(&facts.fleet);
    let Some(before) = latest_part(persist, facts.seats.coordinator, &tags).await else {
        write_run_part(persist, (def, facts), None).await;
        return;
    };
    let body = carried(run_part(def, facts), Some(&before));
    if before.body == body {
        return;
    }
    let Some(now) = write_run_part(persist, (def, facts), Some(&before)).await else {
        return;
    };
    let differing = lines_that_differ(&before.body, &body);
    let message = change_mail(
        "the run's part of the briefs",
        (before.version, now.version),
        &differing,
        now.note,
    );
    for seat in seats {
        let key = format!("runpart:{seat}:{}", now.version);
        mail_once(persist, facts, (key, *seat), &message).await;
    }
}

async fn refresh_task_part(
    persist: &PersistHandle,
    (def, facts): (&FleetDefinition, &FleetFacts),
    (task_id, session): (&str, Uuid),
) {
    let tags = task_tags(&facts.fleet, task_id);
    let Some(before) = latest_part(persist, session, &tags).await else {
        write_task_part(persist, (def, facts), (task_id, session), None).await;
        return;
    };
    let Some(fresh) = task_part(def, task_id) else {
        return;
    };
    let body = carried(fresh, Some(&before));
    if before.body == body {
        return;
    }
    let Some(now) = write_task_part(persist, (def, facts), (task_id, session), Some(&before)).await
    else {
        return;
    };
    let message = change_mail(
        &format!("your task {task_id}"),
        (before.version, now.version),
        &lines_that_differ(&before.body, &body),
        now.note,
    );
    let key = format!("taskpart:{task_id}:{}", now.version);
    mail_once(persist, facts, (key, session), &message).await;
}

pub async fn sync(
    persist: &PersistHandle,
    (def, facts): (&FleetDefinition, &FleetFacts),
    open_seats: &[Uuid],
) {
    if !registered(persist, facts.seats.coordinator, Duration::ZERO).await {
        return;
    }
    refresh_run_part(persist, (def, facts), open_seats).await;
    for (task_id, task) in &def.tasks {
        let Some(session) = task
            .session
            .as_deref()
            .and_then(|s| s.trim().parse::<Uuid>().ok())
        else {
            continue;
        };
        if !open_seats.contains(&session) || !registered(persist, session, Duration::ZERO).await {
            continue;
        }
        refresh_task_part(persist, (def, facts), (task_id, session)).await;
    }
}
