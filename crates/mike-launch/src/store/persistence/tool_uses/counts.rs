use serde::Serialize;
use sqlx::{Postgres, QueryBuilder};

use super::{ToolUsesFilter, push_scope_filters, push_use_filters};
use crate::store::persistence::PersistError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolUseGroup {
    Tool,
    Session,
    Day,
    Server,
}

impl ToolUseGroup {
    pub const NAMES: &'static str = "tool, session, day or server";

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "tool" => Some(Self::Tool),
            "session" => Some(Self::Session),
            "day" => Some(Self::Day),
            "server" => Some(Self::Server),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tool => "tool",
            Self::Session => "session",
            Self::Day => "day",
            Self::Server => "server",
        }
    }

    fn key_sql(self) -> &'static str {
        match self {
            Self::Tool => "tu.tool_name",
            Self::Session => "s.session_uuid::text",
            Self::Day => "to_char(tu.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD')",
            Self::Server => {
                "CASE WHEN tu.tool_name LIKE 'mcp\\_\\_%' \
                 THEN split_part(tu.tool_name, '__', 2) ELSE 'built-in' END"
            }
        }
    }
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct ToolUseCount {
    pub key: String,
    pub count: i64,
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn count_tool_uses_by(
        self,
        filter: &ToolUsesFilter,
        group: ToolUseGroup,
    ) -> Result<Vec<ToolUseCount>, PersistError> {
        crate::metrics::time_db("count_tool_uses_by", async move {
            let session = match filter.session_id.as_deref() {
                Some(raw) => match self.resolve_session_db_id(raw).await? {
                    Some(id) => Some(id),
                    None => return Ok(Vec::new()),
                },
                None => None,
            };
            let mut qb = QueryBuilder::<Postgres>::new("SELECT ");
            qb.push(group.key_sql())
                .push(" AS key, count(*) AS count FROM tools.uses tu");
            if group == ToolUseGroup::Session {
                qb.push(" JOIN sessions.sessions s ON s.id = tu.session_id");
            }
            qb.push(" WHERE TRUE");
            push_use_filters(&mut qb, filter, session);
            push_scope_filters(&mut qb, filter);
            qb.push(" GROUP BY 1 ORDER BY 2 DESC, 1");
            Ok(qb
                .build_query_as::<ToolUseCount>()
                .fetch_all(&self.pool)
                .await?)
        })
        .await
    }
}
