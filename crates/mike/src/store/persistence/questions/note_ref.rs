use uuid::Uuid;

use super::types::NoteRef;
use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn note_ref(&self, note: Uuid) -> Result<Option<NoteRef>, PersistError> {
        crate::metrics::time_db("questions_note_ref", async move {
            let row = sqlx::query_file!("src/store/persistence/questions/note_ref.sql", note)
                .fetch_optional(&self.pool)
                .await?;
            Ok(row.map(|r| NoteRef {
                note_uuid: r.note_uuid,
                author: r.author,
                title: r.title,
                body: r.body,
                tags: r.tags,
                version: r.version,
            }))
        })
        .await
    }
}
