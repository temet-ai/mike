use uuid::Uuid;

use super::types::QuestionRow;
use crate::store::persistence::PersistError;

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn question_by_uuid(
        self,
        question: Uuid,
    ) -> Result<Option<QuestionRow>, PersistError> {
        crate::metrics::time_db("questions_by_uuid", async move {
            let row = sqlx::query_file_as!(
                QuestionRow,
                "src/store/persistence/questions/by_uuid.sql",
                question
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub async fn open_questions_for(self, session: Uuid) -> Result<Vec<QuestionRow>, PersistError> {
        crate::metrics::time_db("questions_open_for_session", async move {
            let rows = sqlx::query_file_as!(
                QuestionRow,
                "src/store/persistence/questions/open_for_session.sql",
                session
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn open_questions_in_run(self, run: &str) -> Result<Vec<QuestionRow>, PersistError> {
        crate::metrics::time_db("questions_open_in_run", async move {
            let rows = sqlx::query_file_as!(
                QuestionRow,
                "src/store/persistence/questions/open_in_run.sql",
                run
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn escalate_due_questions(
        self,
        run: &str,
        minutes: i32,
    ) -> Result<Vec<QuestionRow>, PersistError> {
        crate::metrics::time_db("questions_escalate_due", async move {
            let rows = sqlx::query_file_as!(
                QuestionRow,
                "src/store/persistence/questions/escalate_due.sql",
                run,
                minutes
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn questions_about_note(self, note: Uuid) -> Result<Vec<QuestionRow>, PersistError> {
        crate::metrics::time_db("questions_about_note", async move {
            let rows = sqlx::query_file_as!(
                QuestionRow,
                "src/store/persistence/questions/about_note.sql",
                note
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
