use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder};

use super::pagination::limit_to_i64;
use crate::store::persistence::PersistError;
use crate::types::SortOrder;

pub(crate) struct DiagnosticAggregateRows {
    pub(crate) by_category: Vec<(String, i64)>,
    pub(crate) by_analyzer: Vec<(String, i64)>,
}

pub(crate) async fn aggregate_diagnostic_stats(
    pool: &PgPool,
    project: Option<&str>,
) -> Result<DiagnosticAggregateRows, PersistError> {
    let Some(project) = project else {
        let by_category = sqlx::query!(
            r#"SELECT category AS "category!", COUNT(*)::bigint AS "total!"
               FROM requests.diagnostics
               WHERE created_at > NOW() - INTERVAL '24 hours'
               GROUP BY category"#,
        )
        .fetch_all(pool)
        .await?;

        let by_analyzer = sqlx::query!(
            r#"SELECT analyzer AS "analyzer!", COUNT(*)::bigint AS "total!"
               FROM requests.diagnostics
               WHERE created_at > NOW() - INTERVAL '24 hours'
               GROUP BY analyzer"#,
        )
        .fetch_all(pool)
        .await?;

        return Ok(DiagnosticAggregateRows {
            by_category: by_category
                .into_iter()
                .map(|r| (r.category, r.total))
                .collect(),
            by_analyzer: by_analyzer
                .into_iter()
                .map(|r| (r.analyzer, r.total))
                .collect(),
        });
    };

    let by_category = sqlx::query!(
        r#"SELECT rd.category AS "category!", COUNT(*)::bigint AS "total!"
           FROM requests.diagnostics rd
           JOIN requests.requests r ON r.id = rd.request_id
           JOIN sessions.sessions s ON s.id = r.session_id
           JOIN projects.projects p ON p.id = s.project_id
           WHERE rd.created_at > NOW() - INTERVAL '24 hours'
             AND p.project_path ILIKE '%' || $1 || '%'
           GROUP BY rd.category"#,
        project,
    )
    .fetch_all(pool)
    .await?;

    let by_analyzer = sqlx::query!(
        r#"SELECT rd.analyzer AS "analyzer!", COUNT(*)::bigint AS "total!"
           FROM requests.diagnostics rd
           JOIN requests.requests r ON r.id = rd.request_id
           JOIN sessions.sessions s ON s.id = r.session_id
           JOIN projects.projects p ON p.id = s.project_id
           WHERE rd.created_at > NOW() - INTERVAL '24 hours'
             AND p.project_path ILIKE '%' || $1 || '%'
           GROUP BY rd.analyzer"#,
        project,
    )
    .fetch_all(pool)
    .await?;

    Ok(DiagnosticAggregateRows {
        by_category: by_category
            .into_iter()
            .map(|r| (r.category, r.total))
            .collect(),
        by_analyzer: by_analyzer
            .into_iter()
            .map(|r| (r.analyzer, r.total))
            .collect(),
    })
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct DiagnosticFindingRow {
    pub(crate) request_id: i64,
    pub(crate) session_id: String,
    pub(crate) category: String,
    pub(crate) analyzer: String,
    pub(crate) advice: String,
    pub(crate) matched_text: String,
    pub(crate) match_range: Option<serde_json::Value>,
    pub(crate) other_hits: serde_json::Value,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) request_model: Option<String>,
    pub(crate) request_status: String,
    pub(crate) request_path: String,
    pub(crate) request_human_prompt: String,
    pub(crate) request_created_at: DateTime<Utc>,
    pub(crate) total_rows: i64,
    pub(crate) project_path: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FindingsQuery<'a> {
    pub(crate) offset: usize,
    pub(crate) limit: usize,
    pub(crate) category: Option<&'a str>,
    pub(crate) analyzer: Option<&'a str>,
    pub(crate) session_id: Option<&'a str>,
    pub(crate) project: Option<&'a str>,
    pub(crate) with_total: bool,
    pub(crate) order: SortOrder,
}

struct FindingsFilter<'a> {
    category: Option<&'a str>,
    analyzer: Option<&'a str>,
    session_id: Option<&'a str>,
    project: Option<&'a str>,
}

fn push_findings_where(qb: &mut QueryBuilder<Postgres>, filter: &FindingsFilter<'_>) {
    let mut sep = " WHERE ";
    if let Some(category) = filter.category {
        qb.push(sep)
            .push("rd.category = ")
            .push_bind(category.to_owned());
        sep = " AND ";
    }
    if let Some(analyzer) = filter.analyzer {
        qb.push(sep)
            .push("rd.analyzer = ")
            .push_bind(analyzer.to_owned());
        sep = " AND ";
    }
    if let Some(session_id) = filter.session_id {
        qb.push(sep)
            .push("s.session_uuid = sessions.try_uuid(")
            .push_bind(session_id.to_owned())
            .push("::text)");
        sep = " AND ";
    }
    if let Some(project) = filter.project {
        qb.push(sep)
            .push("p.project_path ILIKE '%' || ")
            .push_bind(project.to_owned())
            .push(" || '%'");
    }
}

pub(crate) async fn list_diagnostic_findings(
    pool: &PgPool,
    query: &FindingsQuery<'_>,
) -> Result<Vec<DiagnosticFindingRow>, PersistError> {
    let filter = FindingsFilter {
        category: query.category,
        analyzer: query.analyzer,
        session_id: query.session_id,
        project: query.project,
    };
    let total_expr = if query.with_total {
        "COUNT(*) OVER ()::bigint AS total_rows"
    } else {
        "0::bigint AS total_rows"
    };
    let mut qb = QueryBuilder::<Postgres>::new(
        "SELECT rd.request_id,
                s.session_uuid::text AS session_id,
                rd.category,
                rd.analyzer,
                rd.advice,
                rd.matched_text,
                rd.match_range,
                rd.other_hits,
                rd.created_at,
                r.model            AS request_model,
                rs.code            AS request_status,
                r.path             AS request_path,
                r.human_prompt     AS request_human_prompt,
                r.created_at       AS request_created_at, ",
    );
    qb.push(total_expr).push(
        ",
                p.project_path     AS project_path
         FROM requests.diagnostics rd
         INNER JOIN requests.requests r ON r.id = rd.request_id
         INNER JOIN requests.statuses rs ON rs.id = r.status_id
         LEFT JOIN sessions.sessions s ON s.id = r.session_id
         LEFT JOIN projects.projects p ON p.id = s.project_id",
    );
    push_findings_where(&mut qb, &filter);
    qb.push(" ORDER BY rd.created_at ")
        .push(query.order.sql())
        .push(" LIMIT ")
        .push_bind(limit_to_i64(query.limit))
        .push(" OFFSET ")
        .push_bind(limit_to_i64(query.offset));
    let rows = qb.build_query_as().fetch_all(pool).await?;
    Ok(rows)
}

pub(crate) async fn count_diagnostic_findings(
    pool: &PgPool,
    category: Option<&str>,
    analyzer: Option<&str>,
    session_id: Option<&str>,
    project: Option<&str>,
) -> Result<i64, PersistError> {
    let filter = FindingsFilter {
        category,
        analyzer,
        session_id,
        project,
    };
    let mut qb = QueryBuilder::<Postgres>::new(
        "SELECT COUNT(*)::bigint
         FROM requests.diagnostics rd
         INNER JOIN requests.requests r ON r.id = rd.request_id
         LEFT JOIN sessions.sessions s ON s.id = r.session_id
         LEFT JOIN projects.projects p ON p.id = s.project_id",
    );
    push_findings_where(&mut qb, &filter);
    let count: i64 = qb.build_query_scalar().fetch_one(pool).await?;
    Ok(count)
}

pub(crate) async fn fetch_session_diagnostic_hit_counts(
    pool: &PgPool,
    session_uuid: uuid::Uuid,
) -> Result<Vec<(String, i64)>, PersistError> {
    let rows = sqlx::query!(
        r#"SELECT rd.analyzer AS "analyzer!", COUNT(*)::bigint AS "hit_count!"
           FROM requests.diagnostics rd
           JOIN requests.requests r ON r.id = rd.request_id
           JOIN sessions.sessions s ON s.id = r.session_id
          WHERE s.session_uuid = $1
          GROUP BY rd.analyzer"#,
        session_uuid,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.analyzer, r.hit_count))
        .collect())
}

pub(crate) async fn fetch_intercepted_count_for_session(
    pool: &PgPool,
    session_uuid: uuid::Uuid,
) -> Result<i64, PersistError> {
    let count = sqlx::query_scalar!(
        r#"SELECT COUNT(*)::bigint AS "count!"
           FROM requests.requests r
           JOIN sessions.sessions s ON s.id = r.session_id
          WHERE s.session_uuid = $1 AND r.intercepted = true"#,
        session_uuid,
    )
    .fetch_one(pool)
    .await?;
    Ok(count)
}
