use uuid::Uuid;

use super::types::{AskedQuestion, QuestionRow};
use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn ask_question(&self, new: &AskedQuestion<'_>) -> Result<QuestionRow, PersistError> {
        if new.question.trim().is_empty() {
            return Err(PersistError::DataInvariant(
                "ask_question: the question is empty".to_owned(),
            ));
        }
        crate::metrics::time_db("questions_ask", async move {
            let row = sqlx::query_file_as!(
                QuestionRow,
                "src/store/persistence/questions/insert.sql",
                new.asker,
                new.run_id,
                new.to_session,
                new.to_task,
                new.about_note,
                new.question,
                new.blocked,
            )
            .fetch_optional(&self.pool)
            .await?;
            match row {
                Some(row) => Ok(row),
                None => Err(self.refusal_of(new).await?),
            }
        })
        .await
    }

    async fn session_known(&self, session: Uuid) -> Result<bool, PersistError> {
        Ok(self.session_row_id(session).await?.is_some())
    }

    async fn refusal_of(&self, new: &AskedQuestion<'_>) -> Result<PersistError, PersistError> {
        if !self.session_known(new.asker).await? {
            return Ok(PersistError::DataInvariant(format!(
                "ask_question: asker session {} not found",
                new.asker
            )));
        }
        let recipient = new
            .to_session
            .map(|s| s.to_string())
            .unwrap_or_else(|| "none".to_owned());
        Ok(PersistError::DataInvariant(format!(
            "ask_question: recipient session {recipient} not found"
        )))
    }
}
