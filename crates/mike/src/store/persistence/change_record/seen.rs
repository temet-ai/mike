use chrono::{DateTime, Utc};

use crate::change_record::{SeenPath, ShellRun, ToolCallNaming};
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq)]
pub struct EditCall {
    pub session_id: i64,
    pub tool_use_id: i64,
    pub tool_name: String,
    pub at: DateTime<Utc>,
    pub ran_from: DateTime<Utc>,
    pub ran_to: Option<DateTime<Utc>>,
    pub failed: Option<bool>,
    pub input: Option<serde_json::Value>,
}

impl EditCall {
    pub fn naming(&self) -> ToolCallNaming {
        ToolCallNaming {
            session_id: self.session_id,
            tool_use_id: self.tool_use_id,
            tool_name: self.tool_name.clone(),
            at: self.at,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NamedShellCall {
    pub session_id: i64,
    pub tool_use_id: i64,
    pub started: DateTime<Utc>,
    pub finished: Option<DateTime<Utc>>,
}

impl PersistHandle {
    pub async fn shell_calls_naming_path(
        &self,
        checkout: &str,
        (relative, absolute): (&str, &str),
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<NamedShellCall>, PersistError> {
        crate::metrics::time_db("change_record_shell_calls_naming_path", async move {
            let rows = sqlx::query_file_as!(
                NamedShellCall,
                "src/store/persistence/change_record/shell_calls_naming_path.sql",
                checkout,
                relative,
                absolute,
                from,
                to,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn paths_seen_by(
        &self,
        session: i64,
        since: DateTime<Utc>,
    ) -> Result<Vec<SeenPath>, PersistError> {
        crate::metrics::time_db("change_record_paths_seen_by", async move {
            let rows = sqlx::query_file_as!(
                SeenPath,
                "src/store/persistence/change_record/paths_seen_by.sql",
                session,
                since,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn tool_calls_naming_path(
        &self,
        path: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<EditCall>, PersistError> {
        crate::metrics::time_db("change_record_tool_calls_naming_path", async move {
            let rows = sqlx::query_file_as!(
                EditCall,
                "src/store/persistence/change_record/tool_calls_naming_path.sql",
                path,
                from,
                to,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn shell_commands_running(
        &self,
        checkout: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<ShellRun>, PersistError> {
        crate::metrics::time_db("change_record_shell_commands_running", async move {
            let rows = sqlx::query_file_as!(
                ShellRun,
                "src/store/persistence/change_record/shell_commands_running.sql",
                checkout,
                from,
                to,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
