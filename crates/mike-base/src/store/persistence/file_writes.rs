use chrono::{DateTime, Utc};

use sqlx::{Postgres, QueryBuilder};

use super::PersistError;

#[derive(Debug, sqlx::FromRow)]
pub struct FileWriteRow {
    pub file_path: String,
    pub session_uuid: String,
    pub request_id: i64,
    pub tool_name: String,
    pub created_at: DateTime<Utc>,
    pub agent_name: Option<String>,
    pub subagent_persona: Option<String>,
    pub line_delta: Option<i32>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct FileWriteSummaryRow {
    pub file_path: String,
    pub session_count: i64,
    pub write_count: i64,
    pub first_write: DateTime<Utc>,
    pub last_write: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct FileWriteFilter<'a> {
    pub project: Option<&'a str>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub path: Option<&'a str>,
    pub collisions_only: bool,
}

fn push_write_filters(qb: &mut QueryBuilder<Postgres>, filter: &FileWriteFilter<'_>) {
    if let Some(project) = filter.project {
        qb.push(" AND p.project_path ILIKE '%' || ")
            .push_bind(project.to_owned())
            .push(" || '%'");
    }
    if let Some(from) = filter.from {
        qb.push(" AND u.created_at >= ").push_bind(from);
    }
    if let Some(to) = filter.to {
        qb.push(" AND u.created_at < ").push_bind(to);
    }
    if let Some(path) = filter.path {
        qb.push(" AND u.file_path ILIKE '%' || ")
            .push_bind(path.to_owned())
            .push(" || '%'");
    }
}

fn push_grouped_writes(qb: &mut QueryBuilder<Postgres>, filter: &FileWriteFilter<'_>) {
    qb.push(
        " FROM tools.uses u
           JOIN requests.requests r ON r.id = u.request_id
      LEFT JOIN projects.projects p ON p.id = r.project_id
          WHERE u.operation = 'write'
            AND u.file_path IS NOT NULL",
    );
    push_write_filters(qb, filter);
    qb.push(" GROUP BY u.file_path");
    if filter.collisions_only {
        qb.push(" HAVING COUNT(DISTINCT r.session_id) > 1");
    }
}

pub async fn summarise_file_writes(
    pool: &sqlx::PgPool,
    filter: &FileWriteFilter<'_>,
    offset: i64,
    limit: i64,
) -> Result<Vec<FileWriteSummaryRow>, PersistError> {
    let mut qb = QueryBuilder::<Postgres>::new(
        "SELECT u.file_path,
                COUNT(DISTINCT r.session_id) AS session_count,
                COUNT(*)                     AS write_count,
                MIN(u.created_at)            AS first_write,
                MAX(u.created_at)            AS last_write",
    );
    push_grouped_writes(&mut qb, filter);
    qb.push(" ORDER BY COUNT(DISTINCT r.session_id) DESC, COUNT(*) DESC, u.file_path OFFSET ")
        .push_bind(offset)
        .push(" LIMIT ")
        .push_bind(limit);
    let rows = qb
        .build_query_as::<FileWriteSummaryRow>()
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn count_file_writes(
    pool: &sqlx::PgPool,
    filter: &FileWriteFilter<'_>,
) -> Result<i64, PersistError> {
    let mut qb = QueryBuilder::<Postgres>::new("SELECT COUNT(*) FROM (SELECT u.file_path");
    push_grouped_writes(&mut qb, filter);
    qb.push(") g");
    let count = qb.build_query_scalar::<i64>().fetch_one(pool).await?;
    Ok(count)
}

pub async fn fetch_writes_for_paths(
    pool: &sqlx::PgPool,
    paths: &[String],
    filter: &FileWriteFilter<'_>,
) -> Result<Vec<FileWriteRow>, PersistError> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut qb = QueryBuilder::<Postgres>::new(
        "SELECT u.file_path,
                s.session_uuid::text AS session_uuid,
                u.request_id,
                u.tool_name,
                u.created_at,
                a.name AS agent_name,
                r.subagent_persona,
                (CASE WHEN u.input->>'new_string' IS NOT NULL
                       AND u.input->>'old_string' IS NOT NULL
                 THEN COALESCE(array_length(string_to_array(u.input->>'new_string', chr(10)), 1), 0)
                    - COALESCE(array_length(string_to_array(u.input->>'old_string', chr(10)), 1), 0)
                 END)::int AS line_delta
           FROM tools.uses u
           JOIN requests.requests r ON r.id = u.request_id
           JOIN sessions.sessions s ON s.id = r.session_id
      LEFT JOIN agents.instances ai ON ai.id = r.agent_instance_id
      LEFT JOIN agents.agents a ON a.id = ai.agent_id
          WHERE u.operation = 'write'
            AND u.file_path = ANY(",
    );
    qb.push_bind(paths.to_vec()).push(")");
    if let Some(from) = filter.from {
        qb.push(" AND u.created_at >= ").push_bind(from);
    }
    if let Some(to) = filter.to {
        qb.push(" AND u.created_at < ").push_bind(to);
    }
    qb.push(" ORDER BY u.file_path, u.created_at, u.id");
    let rows = qb.build_query_as::<FileWriteRow>().fetch_all(pool).await?;
    Ok(rows)
}
