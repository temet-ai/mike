use uuid::Uuid;

use crate::change_record::SessionIdentity;
use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn session_identities(
        &self,
        ids: &[i64],
    ) -> Result<Vec<SessionIdentity>, PersistError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db("change_record_session_identities", async move {
            let rows = sqlx::query_file_as!(
                SessionIdentity,
                "src/store/persistence/change_record/session_identities.sql",
                ids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn session_identity_by_uuid(
        &self,
        uuid: Uuid,
    ) -> Result<Option<SessionIdentity>, PersistError> {
        crate::metrics::time_db("change_record_session_identity_by_uuid", async move {
            let row = sqlx::query_file_as!(
                SessionIdentity,
                "src/store/persistence/change_record/session_identity_by_uuid.sql",
                uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }
}
