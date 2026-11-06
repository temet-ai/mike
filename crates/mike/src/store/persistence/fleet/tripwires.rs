use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug)]
pub(crate) struct ClosedMailAttempt {
    pub id: i64,
    pub sender: Uuid,
    pub recipient: Uuid,
    pub seat: Option<String>,
    pub attempted_at: DateTime<Utc>,
}

impl PersistHandle {
    pub(crate) async fn fleet_closed_mail_attempts(
        &self,
        run: &str,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Vec<ClosedMailAttempt>, PersistError> {
        crate::metrics::time_db("launch_fleet_closed_mail_attempts", async move {
            Ok(sqlx::query_file_as!(
                ClosedMailAttempt,
                "src/store/persistence/fleet/closed_mail_attempts.sql",
                run,
                since,
                until,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
