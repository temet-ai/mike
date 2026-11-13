use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use serde_json::Value;
use uuid::Uuid;

use crate::store::persistence::PersistHandle;
use crate::store::persistence::session_notes::{NewSessionNote, NoteOrder, SessionNoteFilter};

pub const KIND: &str = "note";
const NOTES_READ: i64 = 100;

pub struct RunNote {
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub data: Value,
}

pub fn own_session(run: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("mike-run-notes:{run}").as_bytes(),
    )
}

async fn registered(persist: &PersistHandle, session: Uuid) -> bool {
    matches!(
        persist.resolve_session_db_id(&session.to_string()).await,
        Ok(Some(_))
    )
}

pub async fn session(
    persist: &PersistHandle,
    coordinator: Option<Uuid>,
    repo: &str,
    run: &str,
) -> Option<Uuid> {
    if let Some(coordinator) = coordinator
        && registered(persist, coordinator).await
    {
        return Some(coordinator);
    }
    own_notebook(persist, repo, run).await
}

fn say_not_opened(run: &str, err: &dyn std::fmt::Display) {
    tracing::warn!(%run, error = %err, "fleet: mike could not open its notebook for the run");
}

fn say_not_filed(run: &str, err: &dyn std::fmt::Debug) {
    tracing::warn!(%run, error = ?err, "fleet: mike could not file its notebook for the run under the project");
}

async fn own_notebook(persist: &PersistHandle, repo: &str, run: &str) -> Option<Uuid> {
    let own = own_session(run);
    if let Err(err) = persist
        .launch()
        .ensure_session_exists(&own.to_string())
        .await
    {
        say_not_opened(run, &err);
        return None;
    }
    if let Err(err) = persist.assign_session_project(own, repo).await {
        say_not_filed(run, &err);
    }
    Some(own)
}

fn say_not_written(title: &str, err: &dyn std::fmt::Display) {
    tracing::warn!(%title, error = %err, "fleet: mike could not record a note about the run");
}

pub async fn exists(persist: &PersistHandle, session: Uuid, tags: &[String]) -> bool {
    let Some(last) = tags.last() else {
        return false;
    };
    let filter = SessionNoteFilter {
        kind: Some(KIND),
        status: None,
        tag: Some(last.as_str()),
        q: None,
        order: NoteOrder::NewestFirst,
        limit: NOTES_READ,
        offset: 0,
    };
    match persist.launch().fetch_session_notes(session, filter).await {
        Ok(rows) => rows
            .iter()
            .any(|row| tags.iter().all(|tag| row.tags.contains(tag))),
        Err(_) => false,
    }
}

pub async fn write_once(persist: &PersistHandle, session: Uuid, note: RunNote) -> bool {
    if exists(persist, session, &note.tags).await {
        return false;
    }
    let stored = NewSessionNote {
        kind: Some(KIND),
        title: &note.title,
        body: Some(&note.body),
        status: None,
        tags: Some(note.tags.as_slice()),
        data: Some(&note.data),
    };
    match persist.fleet().create_session_note(session, stored).await {
        Ok(_) => true,
        Err(err) => {
            say_not_written(&note.title, &err);
            false
        }
    }
}
