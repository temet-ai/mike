use chrono::{DateTime, Utc};

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct LessonActionRow {
    pub lesson_id: i64,
    pub text: String,
    pub stage: String,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub project_path: Option<String>,
    pub k_before: Option<i32>,
    pub n_before: Option<i32>,
    pub window_start_before: Option<DateTime<Utc>>,
    pub window_end_before: Option<DateTime<Utc>>,
    pub k_after: Option<i32>,
    pub n_after: Option<i32>,
    pub window_start_after: Option<DateTime<Utc>>,
    pub window_end_after: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct GuardQuestionActionRow {
    pub question: String,
    pub mode: String,
    pub k_holdback: Option<i32>,
    pub n_holdback: Option<i32>,
    pub window_start_holdback: Option<DateTime<Utc>>,
    pub window_end_holdback: Option<DateTime<Utc>>,
    pub k_treated: Option<i32>,
    pub n_treated: Option<i32>,
    pub window_start_treated: Option<DateTime<Utc>>,
    pub window_end_treated: Option<DateTime<Utc>>,
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn list_lesson_actions(
        self,
        project_path: Option<&str>,
    ) -> Result<Vec<LessonActionRow>, PersistError> {
        crate::metrics::time_db("list_lesson_actions", async move {
            let rows = match project_path {
                Some(path) => {
                    sqlx::query_file_as!(
                        LessonActionRow,
                        "src/store/persistence/improvement/lesson_actions_for_project.sql",
                        path,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
                None => {
                    sqlx::query_file_as!(
                        LessonActionRow,
                        "src/store/persistence/improvement/lesson_actions.sql",
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
            };
            Ok(rows)
        })
        .await
    }

    pub async fn get_lesson_action(
        self,
        lesson_id: i64,
    ) -> Result<Option<LessonActionRow>, PersistError> {
        crate::metrics::time_db("get_lesson_action", async move {
            let row = sqlx::query_file_as!(
                LessonActionRow,
                "src/store/persistence/improvement/lesson_action_by_id.sql",
                lesson_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub async fn list_guard_question_actions(
        self,
    ) -> Result<Vec<GuardQuestionActionRow>, PersistError> {
        crate::metrics::time_db("list_guard_question_actions", async move {
            let rows = sqlx::query_file_as!(
                GuardQuestionActionRow,
                "src/store/persistence/improvement/guard_question_actions.sql",
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
