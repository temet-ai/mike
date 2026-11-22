use uuid::Uuid;

use crate::store::persistence::PersistError;

const READ_WAITING_MAIL: &str = include_str!("../sql/read_waiting_mail.sql");
const READ_MAIL_NOTIFIED: &str = include_str!("../sql/read_mail_notified.sql");
const LATEST_RUN: &str = "SELECT max(id) FROM wright.check_runs";

impl crate::store::persistence::web_repo::WebDb<'_> {
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
