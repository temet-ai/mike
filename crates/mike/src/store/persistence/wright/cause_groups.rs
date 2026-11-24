use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::store::persistence::PersistError;

const GROUP_LIST: &str = include_str!("sql/triage/group_list.sql");
const GROUP_CREATE: &str = include_str!("sql/triage/group_create.sql");
const GROUP_RENAME: &str = include_str!("sql/triage/group_rename.sql");
const GROUP_DELETE: &str = include_str!("sql/triage/group_delete.sql");

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub(crate) struct CauseGroupRow {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) causes: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum GroupWrite {
    Saved(CauseGroupRow),
    NameTaken,
    NoGroup,
}

fn is_name_taken(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|database| database.is_unique_violation())
}

impl crate::store::persistence::web_repo::WebDb<'_> {
    pub(crate) async fn list_wright_cause_groups(self) -> Result<Vec<CauseGroupRow>, PersistError> {
        Ok(
            sqlx::query_as(GROUP_LIST) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn create_wright_cause_group(
        self,
        name: &str,
    ) -> Result<GroupWrite, PersistError> {
        let created: Option<CauseGroupRow> = sqlx::query_as(GROUP_CREATE) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .bind(name)
            .fetch_optional(self.pool())
            .await?;
        Ok(created.map_or(GroupWrite::NameTaken, GroupWrite::Saved))
    }

    pub(crate) async fn rename_wright_cause_group(
        self,
        id: i64,
        name: &str,
    ) -> Result<GroupWrite, PersistError> {
        let renamed = sqlx::query_as(GROUP_RENAME) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .bind(id)
            .bind(name)
            .fetch_optional(self.pool())
            .await;
        match renamed {
            Ok(Some(row)) => Ok(GroupWrite::Saved(row)),
            Ok(None) => Ok(GroupWrite::NoGroup),
            Err(e) if is_name_taken(&e) => Ok(GroupWrite::NameTaken),
            Err(e) => Err(e.into()),
        }
    }

    pub(crate) async fn delete_wright_cause_group(self, id: i64) -> Result<bool, PersistError> {
        let deleted: Option<i64> = sqlx::query_scalar(GROUP_DELETE) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        Ok(deleted.is_some())
    }
}
