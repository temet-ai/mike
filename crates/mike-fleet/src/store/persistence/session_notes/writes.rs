use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::sync::Arc;

use uuid::Uuid;

use super::{
    AfterFuture, NewSessionNote, NoteRules, RuleFuture, SessionNoteRow, SessionNoteUpdate,
};
use crate::store::persistence::{PersistError, PersistHandle};

pub struct FleetNoteRules;

impl NoteRules for FleetNoteRules {
    fn validate<'a>(
        &'a self,
        connection: &'a mut sqlx::PgConnection,
        row: &'a SessionNoteRow,
    ) -> RuleFuture<'a> {
        Box::pin(super::obligation::validate(connection, row))
    }

    fn protect_update<'a>(
        &'a self,
        connection: &'a mut sqlx::PgConnection,
        note: Uuid,
        update: SessionNoteUpdate<'a>,
    ) -> RuleFuture<'a> {
        Box::pin(super::obligation::protect_update(connection, note, update))
    }

    fn protect_delete<'a>(
        &'a self,
        connection: &'a mut sqlx::PgConnection,
        note: Uuid,
    ) -> RuleFuture<'a> {
        Box::pin(super::obligation::protect_delete(connection, note))
    }

    fn after_write<'a>(
        &'a self,
        persist: &'a PersistHandle,
        row: &'a SessionNoteRow,
    ) -> AfterFuture<'a> {
        Box::pin(async move {
            crate::launch::fleet::carry::after_note(persist, row.note_uuid, &row.tags, &row.data)
                .await;
            crate::launch::fleet::ruling_notice::after_note(persist, row).await;
        })
    }
}

pub fn install() {
    super::install_rules(Arc::new(FleetNoteRules));
}

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn create_session_note(
        self,
        session_uuid: Uuid,
        note: NewSessionNote<'_>,
    ) -> Result<SessionNoteRow, PersistError> {
        self.launch()
            .create_session_note_with(&FleetNoteRules, session_uuid, note)
            .await
    }

    pub async fn update_session_note(
        self,
        note_uuid: Uuid,
        update: SessionNoteUpdate<'_>,
    ) -> Result<Option<SessionNoteRow>, PersistError> {
        self.launch()
            .update_session_note_with(&FleetNoteRules, note_uuid, update)
            .await
    }

    pub async fn delete_session_note(self, note_uuid: Uuid) -> Result<bool, PersistError> {
        self.launch()
            .delete_session_note_with(&FleetNoteRules, note_uuid)
            .await
    }
}
