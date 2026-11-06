use uuid::Uuid;

use super::{InsertedTemplate, NewTemplate, TemplateRow};
use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn insert_template(
        &self,
        new: &NewTemplate,
    ) -> Result<InsertedTemplate, PersistError> {
        crate::metrics::time_db("launch_insert_template", async move {
            let mut tx = self.pool.begin().await?;
            let existing = sqlx::query_file!(
                "src/store/persistence/launch/find_live_template_id_by_name.sql",
                new.name,
            )
            .fetch_optional(&mut *tx)
            .await?;
            let claude_args = serde_json::Value::from(new.claude_args.clone());
            let id = sqlx::query_file_scalar!(
                "src/store/persistence/launch/insert_template.sql",
                new.name,
                new.session_uuid,
                new.folder,
                new.backend,
                new.model,
                new.agent,
                claude_args,
            )
            .fetch_one(&mut *tx)
            .await?;
            let replaced_session = match &existing {
                Some(row) => {
                    sqlx::query_file!(
                        "src/store/persistence/launch/supersede_template.sql",
                        row.id,
                        id,
                    )
                    .execute(&mut *tx)
                    .await?;
                    Some(row.session_uuid)
                }
                None => None,
            };
            tx.commit().await?;
            Ok(InsertedTemplate {
                id,
                replaced_session,
            })
        })
        .await
    }

    pub async fn find_live_template_by_name(
        &self,
        name: &str,
    ) -> Result<Option<TemplateRow>, PersistError> {
        crate::metrics::time_db("launch_find_live_template_by_name", async move {
            let row = sqlx::query_file_as!(
                TemplateRow,
                "src/store/persistence/launch/find_live_template_by_name.sql",
                name,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub async fn find_live_template_by_session(
        &self,
        session: Uuid,
    ) -> Result<Option<TemplateRow>, PersistError> {
        crate::metrics::time_db("launch_find_live_template_by_session", async move {
            let row = sqlx::query_file_as!(
                TemplateRow,
                "src/store/persistence/launch/find_live_template_by_session.sql",
                session,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub async fn list_live_templates(&self) -> Result<Vec<TemplateRow>, PersistError> {
        crate::metrics::time_db("launch_list_live_templates", async move {
            let rows = sqlx::query_file_as!(
                TemplateRow,
                "src/store/persistence/launch/list_live_templates.sql",
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn close_template(&self, name: &str) -> Result<bool, PersistError> {
        crate::metrics::time_db("launch_close_template", async move {
            let hit =
                sqlx::query_file_scalar!("src/store/persistence/launch/close_template.sql", name)
                    .fetch_optional(&self.pool)
                    .await?;
            Ok(hit.is_some())
        })
        .await
    }
}
