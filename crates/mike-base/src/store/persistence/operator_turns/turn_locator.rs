use chrono::{DateTime, Utc};

use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct TurnLocator {
    pub turn_id: i64,
    pub session_db_id: i64,
    pub session_uuid: String,
    pub start_request_id: i64,
    pub opened_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

impl PersistHandle {
    pub async fn locate_turn(&self, turn_id: i64) -> Result<Option<TurnLocator>, PersistError> {
        crate::metrics::time_db("locate_turn", async move {
            let row = sqlx::query_as!(
                TurnLocator,
                r#"SELECT t.id AS turn_id, t.session_id AS session_db_id,
                          s.session_uuid::text AS "session_uuid!", t.start_request_id,
                          t.opened_by, t.created_at, t.closed_at
                     FROM memory.operator_turns t
                     JOIN sessions.sessions s ON s.id = t.session_id
                    WHERE t.id = $1"#,
                turn_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }
}
