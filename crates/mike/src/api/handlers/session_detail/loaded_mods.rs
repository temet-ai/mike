use crate::store::persistence::PersistHandle;
use crate::store::persistence::session_notes::{NoteOrder, SessionNoteFilter, SessionNoteRow};

pub(super) const MODS_NOTE_KIND: &str = "mods";

pub(super) fn latest_mods_listing(newest_first: Vec<SessionNoteRow>) -> Option<serde_json::Value> {
    newest_first
        .into_iter()
        .find(|row| row.kind == MODS_NOTE_KIND && row.data.is_object())
        .map(|row| row.data)
}

pub(super) async fn fetch_loaded_mods(
    handle: &PersistHandle,
    session_id: &str,
) -> Option<serde_json::Value> {
    let session_uuid = uuid::Uuid::parse_str(session_id).ok()?;
    let filter = SessionNoteFilter {
        kind: Some(MODS_NOTE_KIND),
        status: None,
        tag: None,
        q: None,
        order: NoteOrder::NewestFirst,
        limit: 1,
        offset: 0,
    };
    match handle.fetch_session_notes(session_uuid, filter).await {
        Ok(rows) => latest_mods_listing(rows),
        Err(e) => {
            tracing::warn!(
                session_id = %session_id,
                error = %e,
                "get_session: loaded mods fetch failed — omitting from response"
            );
            None
        }
    }
}

#[cfg(test)]
#[path = "loaded_mods_tests.rs"]
mod tests;
