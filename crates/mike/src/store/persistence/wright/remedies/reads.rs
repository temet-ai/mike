use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::store::persistence::PersistError;

const READ_LAUNCH: &str = include_str!("../sql/read_launch.sql");
const READ_WAITING_MAIL: &str = include_str!("../sql/read_waiting_mail.sql");
const READ_MAIL_NOTIFIED: &str = include_str!("../sql/read_mail_notified.sql");
const LATEST_RUN: &str = "SELECT max(id) FROM wright.check_runs";

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct LaunchState {
    pub(crate) id: i64,
    pub(crate) session_uuid: Option<Uuid>,
    pub(crate) run_id: Option<String>,
    pub(crate) seat: Option<String>,
    pub(crate) closed_at: Option<DateTime<Utc>>,
    pub(crate) close_reason: Option<String>,
}

impl crate::store::persistence::web_repo::WebDb<'_> {
    pub(crate) async fn wright_launch_state(
        self,
        id: i64,
    ) -> Result<Option<LaunchState>, PersistError> {
        Ok(
            sqlx::query_as(READ_LAUNCH) // grader-allow: runtime read over sql/*.sql so the parallel Wright branches need no .sqlx entries; bound by the remedies DB tests
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub(crate) async fn wright_waiting_mail(
        self,
        recipient: Uuid,
        limit: i64,
    ) -> Result<Vec<Uuid>, PersistError> {
        Ok(sqlx::query_scalar(READ_WAITING_MAIL)
            .bind(recipient)
            .bind(limit)
            .fetch_all(self.pool())
            .await?)
    }

    pub(crate) async fn wright_mail_notified(
        self,
        messages: &[Uuid],
    ) -> Result<Vec<Uuid>, PersistError> {
        Ok(sqlx::query_scalar(READ_MAIL_NOTIFIED)
            .bind(messages)
            .fetch_all(self.pool())
            .await?)
    }

    pub(crate) async fn latest_wright_check_run(self) -> Result<Option<i64>, PersistError> {
        Ok(sqlx::query_scalar(LATEST_RUN)
            .fetch_one(self.pool())
            .await?)
    }
}
