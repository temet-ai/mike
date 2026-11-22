use uuid::Uuid;

use super::types::QuestionRow;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistError;
use crate::store::persistence::feed_memo::{Memo, Scope};

struct NoOpenQuestions(Memo<()>);

impl Default for NoOpenQuestions {
    fn default() -> Self {
        Self(Memo::new(Scope {
            tables: &[],
            per_session: &[RowChangedTable::WorkspaceQuestions],
        }))
    }
}

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
        let none = self.memo::<NoOpenQuestions>();
        let memo = self
            .feed_memos
            .fed_feed()
            .and(self.feed_memos.bound(&none.0));
        if memo.as_ref().and_then(|memo| memo.get(session)).is_some() {
            return Ok(Vec::new());
        }
        let ticket = memo.as_ref().map(|memo| memo.ticket());
        let rows = self.read_open_questions(session).await?;
        if let (Some(memo), Some(ticket), true) = (memo, ticket, rows.is_empty()) {
            memo.remember(session, ticket, ());
        }
        Ok(rows)
    }

    async fn read_open_questions(self, session: Uuid) -> Result<Vec<QuestionRow>, PersistError> {
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
