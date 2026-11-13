use serde::Serialize;
use sqlx::{Postgres, QueryBuilder};

use super::PersistError;
use super::keyset::{CappedCount, KeysetCursor, probe_limit, split_page};
use crate::types::SortOrder;

mod seat_reads;

pub use seat_reads::SessionToolRead;

pub struct ToolUsesFilter {
    pub project: Option<String>,
    pub tool_name: Option<String>,
    pub session_id: Option<String>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub errors_only: bool,
    pub limit: i64,
    pub offset: i64,
    pub cursor: Option<KeysetCursor>,
    pub order: SortOrder,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct ToolUsesFeedRow {
    pub use_id: i64,
    pub wire_tool_use_id: String,
    pub request_id: i64,
    pub session_id: String,
    pub project_path: Option<String>,
    pub tool_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub ord: i32,
    pub input: Option<serde_json::Value>,
    pub file_path: Option<String>,
    pub is_error: bool,
    pub result_id: Option<i64>,
    pub error_content: Option<serde_json::Value>,
    pub error_content_bytes: Option<i64>,
    pub error_content_sha256: Option<String>,
}

pub struct ToolUsesPage {
    pub total: CappedCount,
    pub rows: Vec<ToolUsesFeedRow>,
    pub next_cursor: Option<KeysetCursor>,
}

const USES_PAGE_SELECT: &str = "SELECT tu.id AS use_id,
       tu.wire_tool_use_id,
       tu.request_id,
       s.session_uuid::text AS session_id,
       p.project_path AS project_path,
       tu.tool_name,
       tu.created_at,
       tu.ord,
       tu.input,
       tu.file_path,
       COALESCE(r.is_error, FALSE) AS is_error,
       r.id AS result_id,
       CASE WHEN r.is_error THEN r.content END AS error_content,
       CASE WHEN r.is_error THEN r.content_bytes END AS error_content_bytes,
       CASE WHEN r.is_error THEN r.content_sha256 END AS error_content_sha256
  FROM tools.uses tu
  JOIN sessions.sessions s ON s.id = tu.session_id
  JOIN requests.requests rq ON rq.id = tu.request_id
  LEFT JOIN projects.projects p ON p.id = rq.project_id
  LEFT JOIN tools.results r ON r.tool_use_id = tu.id
 WHERE TRUE";

fn push_use_filters(
    qb: &mut QueryBuilder<Postgres>,
    filter: &ToolUsesFilter,
    session: Option<i64>,
) {
    if let Some(since) = filter.since {
        qb.push(" AND tu.created_at >= ").push_bind(since);
    }
    if let Some(tool_name) = &filter.tool_name {
        qb.push(" AND tu.tool_name = ").push_bind(tool_name.clone());
    }
    if let Some(session) = session {
        qb.push(" AND tu.session_id = ").push_bind(session);
    }
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn list_tool_uses(
        self,
        filter: &ToolUsesFilter,
    ) -> Result<ToolUsesPage, PersistError> {
        crate::metrics::time_db("list_tool_uses", async move {
            let session = match filter.session_id.as_deref() {
                Some(raw) => match self.resolve_session_db_id(raw).await? {
                    Some(id) => Some(id),
                    None => {
                        return Ok(ToolUsesPage {
                            total: CappedCount::from_probe(0),
                            rows: Vec::new(),
                            next_cursor: None,
                        });
                    }
                },
                None => None,
            };
            let total = self.count_tool_uses(filter, session).await?;
            let rows = self.tool_uses_page(filter, session).await?;
            let (rows, next_cursor) = split_page(rows, filter.limit, |row| KeysetCursor {
                at: row.created_at,
                id: row.use_id,
            });
            Ok(ToolUsesPage {
                total,
                rows,
                next_cursor,
            })
        })
        .await
    }

    async fn count_tool_uses(
        self,
        filter: &ToolUsesFilter,
        session: Option<i64>,
    ) -> Result<CappedCount, PersistError> {
        let mut qb = QueryBuilder::<Postgres>::new(
            "SELECT count(*) FROM (SELECT 1 FROM tools.uses tu WHERE TRUE",
        );
        push_use_filters(&mut qb, filter, session);
        if let Some(project) = &filter.project {
            qb.push(
                " AND EXISTS (SELECT 1 FROM requests.requests rq
                   JOIN projects.projects p ON p.id = rq.project_id
                  WHERE rq.id = tu.request_id AND p.project_path = ",
            )
            .push_bind(project.clone())
            .push(")");
        }
        if filter.errors_only {
            qb.push(" AND tu.id IN (SELECT r.tool_use_id FROM tools.results r WHERE r.is_error)");
        }
        qb.push(" LIMIT ")
            .push_bind(probe_limit(super::keyset::COUNT_CAP))
            .push(") capped");
        let probed = qb.build_query_scalar::<i64>().fetch_one(&self.pool).await?;
        Ok(CappedCount::from_probe(probed))
    }

    async fn tool_uses_page(
        self,
        filter: &ToolUsesFilter,
        session: Option<i64>,
    ) -> Result<Vec<ToolUsesFeedRow>, PersistError> {
        let mut qb = QueryBuilder::<Postgres>::new(USES_PAGE_SELECT);
        push_use_filters(&mut qb, filter, session);
        if let Some(project) = &filter.project {
            qb.push(" AND p.project_path = ").push_bind(project.clone());
        }
        if filter.errors_only {
            qb.push(" AND r.is_error");
        }
        let (cmp, direction) = match filter.order {
            SortOrder::Desc => ("<", "DESC"),
            SortOrder::Asc => (">", "ASC"),
        };
        if let Some(cursor) = filter.cursor {
            qb.push(" AND (tu.created_at, tu.id) ")
                .push(cmp)
                .push(" (")
                .push_bind(cursor.at)
                .push(", ")
                .push_bind(cursor.id)
                .push(")");
        }
        qb.push(" ORDER BY tu.created_at ")
            .push(direction)
            .push(", tu.id ")
            .push(direction);
        if filter.cursor.is_none() && filter.offset > 0 {
            qb.push(" OFFSET ").push_bind(filter.offset);
        }
        qb.push(" LIMIT ").push_bind(probe_limit(filter.limit));
        let rows = qb
            .build_query_as::<ToolUsesFeedRow>()
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    pub(crate) async fn list_tool_use_facets(
        self,
    ) -> Result<(Vec<String>, Vec<String>), PersistError> {
        crate::metrics::time_db("list_tool_use_facets", async move {
            let projects = sqlx::query_scalar!(
                r#"SELECT DISTINCT p.project_path AS "project_path!"
                   FROM tools.uses tu
                   JOIN requests.requests rq ON rq.id = tu.request_id
                   JOIN projects.projects p  ON p.id  = rq.project_id
                  WHERE p.project_path IS NOT NULL
                  ORDER BY p.project_path ASC"#,
            )
            .fetch_all(&self.pool)
            .await?;

            let tool_names = sqlx::query_scalar!(
                r#"SELECT DISTINCT tu.tool_name AS "tool_name!"
                   FROM tools.uses tu
                  WHERE tu.tool_name IS NOT NULL
                  ORDER BY tu.tool_name ASC"#,
            )
            .fetch_all(&self.pool)
            .await?;

            Ok((projects, tool_names))
        })
        .await
    }
}
