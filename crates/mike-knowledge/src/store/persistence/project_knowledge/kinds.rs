use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Postgres;

use crate::store::persistence::PersistError;

const REFUSAL_CODE: &str = "MK400";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindRefusal {
    pub message: String,
    pub field: Option<String>,
}

impl std::fmt::Display for KindRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum KindError {
    #[error("{0}")]
    Refused(KindRefusal),
    #[error(transparent)]
    Persist(#[from] PersistError),
}

fn refusal_of(error: &sqlx::Error) -> Option<KindRefusal> {
    let db = error.as_database_error()?;
    if db.code().as_deref() != Some(REFUSAL_CODE) {
        return None;
    }
    let field = db
        .try_downcast_ref::<sqlx::postgres::PgDatabaseError>()
        .and_then(|pg| pg.detail())
        .map(str::to_owned);
    Some(KindRefusal {
        message: db.message().to_owned(),
        field,
    })
}

impl From<sqlx::Error> for KindError {
    fn from(error: sqlx::Error) -> Self {
        match refusal_of(&error) {
            Some(refusal) => KindError::Refused(refusal),
            None => KindError::Persist(PersistError::from(error)),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct KindDefinition {
    pub kind: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub source_table: Option<String>,
    pub key_column: Option<String>,
    pub title_column: Option<String>,
    pub body_columns: Option<Vec<String>>,
    pub project_column: Option<String>,
    pub watch_columns: Option<Vec<String>>,
    pub settle_secs: Option<i32>,
    pub view: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct KindUpdate {
    pub enabled: Option<bool>,
    pub settle_secs: Option<i32>,
    pub watch_columns: Option<Vec<String>>,
    pub label: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct KindPreview {
    pub reference: String,
    pub header: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct KnowledgeKindRow {
    pub kind: String,
    pub label: String,
    pub description: Option<String>,
    pub body_view: String,
    pub settle_secs: i32,
    pub enabled: bool,
    pub built_in: bool,
    pub generated_view: bool,
    pub source_table: Option<String>,
    pub key_column: Option<String>,
    pub title_column: Option<String>,
    pub body_columns: Option<Vec<String>>,
    pub project_column: Option<String>,
    pub created_at: DateTime<Utc>,
    pub items: i64,
    pub embedded: i64,
    pub waiting: i64,
    pub failed: i64,
    pub newest_error: Option<String>,
    pub newest_error_at: Option<DateTime<Utc>>,
    pub oldest_waiting_ready_at: Option<DateTime<Utc>>,
    pub behind: bool,
    pub watches: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeTable {
    pub name: String,
    pub columns: sqlx::types::Json<Vec<TableColumn>>,
}

async fn define_with<'e, E>(executor: E, definition: &KindDefinition) -> Result<String, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    sqlx::query_scalar::<_, String>(
        "SELECT memory.define_knowledge_kind($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(&definition.kind)
    .bind(&definition.label)
    .bind(&definition.description)
    .bind(&definition.source_table)
    .bind(&definition.key_column)
    .bind(&definition.title_column)
    .bind(definition.body_columns.as_deref())
    .bind(&definition.project_column)
    .bind(definition.watch_columns.as_deref())
    .bind(definition.settle_secs)
    .bind(&definition.view)
    .fetch_one(executor)
    .await
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn list_knowledge_kinds(self) -> Result<Vec<KnowledgeKindRow>, PersistError> {
        Ok(sqlx::query_file_as!(
            KnowledgeKindRow,
            "src/store/persistence/project_knowledge/kinds_list.sql"
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn list_knowledge_tables(self) -> Result<Vec<KnowledgeTable>, PersistError> {
        Ok(sqlx::query_file_as!(
            KnowledgeTable,
            "src/store/persistence/project_knowledge/kinds_tables.sql"
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn known_knowledge_kinds(self) -> Result<Vec<String>, PersistError> {
        Ok(
            sqlx::query_scalar::<_, String>(
                "SELECT kind FROM memory.knowledge_kinds ORDER BY kind",
            )
            .fetch_all(self.pool())
            .await?,
        )
    }

    pub async fn define_knowledge_kind(
        self,
        definition: &KindDefinition,
    ) -> Result<String, KindError> {
        let kind = define_with(self.pool(), definition).await?;
        self.forget_knowledge_coverage();
        Ok(kind)
    }

    pub async fn preview_knowledge_kind(
        self,
        definition: &KindDefinition,
    ) -> Result<Option<KindPreview>, KindError> {
        let mut tx = self.pool().begin().await?;
        let kind = define_with(&mut *tx, definition).await?;
        let preview = sqlx::query_file_as!(
            KindPreview,
            "src/store/persistence/project_knowledge/kinds_preview.sql",
            kind
        )
        .fetch_optional(&mut *tx)
        .await?;
        tx.rollback().await?;
        Ok(preview)
    }

    pub async fn update_knowledge_kind(
        self,
        kind: &str,
        update: &KindUpdate,
    ) -> Result<(), KindError> {
        sqlx::query!(
            "SELECT memory.update_knowledge_kind($1, $2, $3, $4, $5, $6)",
            kind,
            update.enabled,
            update.settle_secs,
            update.watch_columns.as_deref(),
            update.label.as_deref(),
            update.description.as_deref()
        )
        .execute(self.pool())
        .await?;
        self.forget_knowledge_coverage();
        Ok(())
    }

    pub async fn delete_knowledge_kind(self, kind: &str) -> Result<(), KindError> {
        sqlx::query!("SELECT memory.delete_knowledge_kind($1)", kind)
            .execute(self.pool())
            .await?;
        self.forget_knowledge_coverage();
        Ok(())
    }
}
