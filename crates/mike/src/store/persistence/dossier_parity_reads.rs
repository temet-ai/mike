use std::collections::HashMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::PersistError;

#[derive(Debug)]
pub struct DispositionRequestRow {
    pub request_id: i64,
    pub session_id: i64,
    pub created_at: DateTime<Utc>,
    pub origin: String,
    pub turn_type: String,
    pub author: String,
    pub has_agent_id: bool,
    pub human_prompt: String,
    pub last_user_message: String,
    pub opened: bool,
    pub closed_turn: bool,
    pub attached: bool,
}

impl crate::store::persistence::dossier_repo::DossierDb<'_> {
    pub async fn session_db_ids_by_uuid(
        self,
        session_uuids: &[Uuid],
    ) -> Result<HashMap<Uuid, i64>, PersistError> {
        let mut found = HashMap::with_capacity(session_uuids.len());
        let mut missing = Vec::new();
        for session_uuid in session_uuids {
            match self.id_cache.session(session_uuid) {
                Some(id) => {
                    found.insert(*session_uuid, id);
                }
                None => missing.push(*session_uuid),
            }
        }
        if missing.is_empty() {
            return Ok(found);
        }
        crate::metrics::time_db("session_db_ids_by_uuid", async move {
            let epoch = self.id_cache.session_epoch();
            let rows = sqlx::query!(
                "SELECT id, session_uuid FROM sessions.sessions WHERE session_uuid = ANY($1)",
                &missing,
            )
            .fetch_all(&self.pool)
            .await?;
            for row in rows {
                self.id_cache
                    .remember_session(epoch, row.session_uuid, row.id);
                found.insert(row.session_uuid, row.id);
            }
            Ok(found)
        })
        .await
    }

    pub async fn list_disposition_requests(
        self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<DispositionRequestRow>, PersistError> {
        crate::metrics::time_db("list_disposition_requests", async move {
            let rows = sqlx::query_file_as!(
                DispositionRequestRow,
                "src/store/persistence/dossier_parity_dispositions.sql",
                from,
                to,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
