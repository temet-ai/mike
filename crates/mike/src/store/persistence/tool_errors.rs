use serde::Serialize;
use sqlx::{Postgres, QueryBuilder};

use super::keyset::{COUNT_CAP, CappedCount, KeysetCursor, probe_limit, split_page};
use super::{PersistError, PersistHandle};
use crate::types::SortOrder;

pub struct ToolErrorFilter {
    pub project: Option<String>,
    pub tool_name: Option<String>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: i64,
    pub offset: i64,
    pub cursor: Option<KeysetCursor>,
    pub order: SortOrder,
}

#[derive(sqlx::FromRow, Serialize)]
pub struct ToolErrorRow {
    pub result_id: i64,
    pub request_id: i64,
    pub session_id: String,
    pub project_path: Option<String>,
    pub tool_use_id: String,
    pub tool_name: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub content_sha256: Option<String>,
    pub content_bytes: Option<i64>,
    pub content: serde_json::Value,
}

pub struct ToolErrorPage {
    pub total: CappedCount,
    pub rows: Vec<ToolErrorRow>,
    pub next_cursor: Option<KeysetCursor>,
}

const ERRORS_PAGE_SELECT: &str = "SELECT r.id AS result_id,
       rq.id AS request_id,
       s.session_uuid::text AS session_id,
       p.project_path AS project_path,
       tu.wire_tool_use_id AS tool_use_id,
       tu.tool_name AS tool_name,
       r.created_at,
       r.content_sha256 AS content_sha256,
       r.content_bytes AS content_bytes,
       r.content
  FROM tools.results r
  JOIN tools.uses tu ON tu.id = r.tool_use_id
  JOIN requests.requests rq ON rq.id = tu.request_id
  JOIN sessions.sessions s ON s.id = rq.session_id
  LEFT JOIN projects.projects p ON p.id = rq.project_id
 WHERE r.is_error";

fn push_error_filters(qb: &mut QueryBuilder<Postgres>, filter: &ToolErrorFilter) {
    if let Some(since) = filter.since {
        qb.push(" AND r.created_at >= ").push_bind(since);
    }
    if let Some(until) = filter.until {
        qb.push(" AND r.created_at < ").push_bind(until);
    }
    if let Some(project) = &filter.project {
        qb.push(" AND p.project_path ILIKE '%' || ")
            .push_bind(project.clone())
            .push(" || '%'");
    }
    if let Some(tool_name) = &filter.tool_name {
        qb.push(" AND tu.tool_name = ").push_bind(tool_name.clone());
    }
}

impl PersistHandle {
    pub async fn list_tool_errors(
        &self,
        filter: &ToolErrorFilter,
    ) -> Result<ToolErrorPage, PersistError> {
        crate::metrics::time_db("list_tool_errors", async move {
            let probed = self.count_tool_errors(filter).await?;
            let rows = self.tool_errors_page(filter).await?;
            let (rows, next_cursor) = split_page(rows, filter.limit, |row| KeysetCursor {
                at: row.created_at,
                id: row.result_id,
            });
            Ok(ToolErrorPage {
                total: CappedCount::from_probe(probed),
                rows,
                next_cursor,
            })
        })
        .await
    }

    async fn count_tool_errors(&self, filter: &ToolErrorFilter) -> Result<i64, PersistError> {
        let mut qb = QueryBuilder::<Postgres>::new(
            "SELECT count(*) FROM (SELECT 1
               FROM tools.results r
               JOIN tools.uses tu ON tu.id = r.tool_use_id
               JOIN requests.requests rq ON rq.id = tu.request_id
               LEFT JOIN projects.projects p ON p.id = rq.project_id
              WHERE r.is_error",
        );
        push_error_filters(&mut qb, filter);
        qb.push(" LIMIT ")
            .push_bind(probe_limit(COUNT_CAP))
            .push(") capped");
        let probed = qb.build_query_scalar::<i64>().fetch_one(&self.pool).await?;
        Ok(probed)
    }

    async fn tool_errors_page(
        &self,
        filter: &ToolErrorFilter,
    ) -> Result<Vec<ToolErrorRow>, PersistError> {
        let mut qb = QueryBuilder::<Postgres>::new(ERRORS_PAGE_SELECT);
        push_error_filters(&mut qb, filter);
        let (cmp, direction) = match filter.order {
            SortOrder::Desc => ("<", "DESC"),
            SortOrder::Asc => (">", "ASC"),
        };
        if let Some(cursor) = filter.cursor {
            qb.push(" AND (r.created_at, r.id) ")
                .push(cmp)
                .push(" (")
                .push_bind(cursor.at)
                .push(", ")
                .push_bind(cursor.id)
                .push(")");
        }
        qb.push(" ORDER BY r.created_at ")
            .push(direction)
            .push(", r.id ")
            .push(direction);
        if filter.cursor.is_none() && filter.offset > 0 {
            qb.push(" OFFSET ").push_bind(filter.offset);
        }
        qb.push(" LIMIT ").push_bind(probe_limit(filter.limit));
        let rows = qb
            .build_query_as::<ToolErrorRow>()
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    pub(crate) async fn list_tool_error_facets(
        &self,
    ) -> Result<(Vec<String>, Vec<String>), PersistError> {
        crate::metrics::time_db("list_tool_error_facets", async move {
            let projects = sqlx::query_scalar!(
                r#"SELECT DISTINCT p.project_path AS "project_path!"
                   FROM tools.results r
                   JOIN tools.uses tu ON tu.id = r.tool_use_id
                   JOIN requests.requests rq ON rq.id = tu.request_id
                   JOIN projects.projects p ON p.id = rq.project_id
                  WHERE r.is_error = TRUE
                    AND p.project_path IS NOT NULL
                  ORDER BY p.project_path ASC"#,
            )
            .fetch_all(&self.pool)
            .await?;

            let tool_names = sqlx::query_scalar!(
                r#"SELECT DISTINCT tu.tool_name AS "tool_name!"
                   FROM tools.results r
                   JOIN tools.uses tu ON tu.id = r.tool_use_id
                  WHERE r.is_error = TRUE
                    AND tu.tool_name IS NOT NULL
                  ORDER BY tu.tool_name ASC"#,
            )
            .fetch_all(&self.pool)
            .await?;

            Ok((projects, tool_names))
        })
        .await
    }
}
