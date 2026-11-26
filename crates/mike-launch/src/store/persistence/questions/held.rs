use uuid::Uuid;

use super::types::QuestionRow;
use crate::store::persistence::PersistError;

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn assign_held_questions(
        self,
        run: &str,
        seats: &[(String, Uuid)],
    ) -> Result<Vec<QuestionRow>, PersistError> {
        if seats.is_empty() {
            return Ok(Vec::new());
        }
        let (tasks, sessions): (Vec<String>, Vec<Uuid>) = seats.iter().cloned().unzip();
        crate::metrics::time_db("questions_assign_held", async move {
            let rows = sqlx::query_file_as!(
                QuestionRow,
                "src/store/persistence/questions/held.sql",
                run,
                &tasks,
                &sessions,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
