use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder};

use super::pagination::limit_to_i64;
use super::session_aggregates::SessionSummaryRow;
use crate::store::persistence::PersistError;
use crate::types::SortOrder;

pub async fn fetch_session_summary(
    pool: &PgPool,
    session_db_id: i64,
) -> Result<Option<SessionSummaryRow>, PersistError> {
    let row = sqlx::query_file_as!(
        SessionSummaryRow,
        "src/store/persistence/rehydrate/session_summary.sql",
        session_db_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

const RANKED_HEAD_SQL: &str = include_str!("session_ranked_head.sql");
const LISTING_TAIL_SQL: &str = include_str!("session_listing_tail.sql");

#[derive(Default, Clone, Copy)]
pub(super) struct SessionPageFilter<'a> {
    pub(super) date_from: Option<DateTime<Utc>>,
    pub(super) date_to: Option<DateTime<Utc>>,
    pub(super) search: Option<&'a str>,
    pub(super) project: Option<&'a str>,
    pub(super) min_request_count: Option<i64>,
}

fn push_page_where<'a>(qb: &mut QueryBuilder<Postgres>, filter: &SessionPageFilter<'a>) {
    let mut sep = " WHERE ";
    if let Some(from) = filter.date_from {
        qb.push(sep).push("s.last_seen >= ").push_bind(from);
        sep = " AND ";
    }
    if let Some(to) = filter.date_to {
        qb.push(sep).push("s.last_seen < ").push_bind(to);
        sep = " AND ";
    }
    if let Some(search) = filter.search {
        qb.push(sep)
            .push("(s.session_uuid = sessions.try_uuid(")
            .push_bind(search)
            .push(") OR s.title % ")
            .push_bind(search)
            .push(" OR s.title ILIKE '%' || ")
            .push_bind(search)
            .push(" || '%')");
        sep = " AND ";
    }
    if let Some(project) = filter.project {
        qb.push(sep)
            .push("p.project_path ILIKE '%' || ")
            .push_bind(project)
            .push(" || '%'");
        sep = " AND ";
    }
    if let Some(min) = filter.min_request_count.filter(|m| *m > 0) {
        qb.push(sep)
            .push(
                "s.id IN (SELECT cr.session_id FROM requests.requests cr \
                 GROUP BY cr.session_id HAVING COUNT(cr.id) >= ",
            )
            .push_bind(min)
            .push(")");
    }
}

pub(super) async fn list_sessions_page(
    pool: &PgPool,
    filter: SessionPageFilter<'_>,
    offset: usize,
    limit: usize,
    order: SortOrder,
) -> Result<Vec<SessionSummaryRow>, PersistError> {
    let mut qb = QueryBuilder::<Postgres>::new(RANKED_HEAD_SQL);
    push_page_where(&mut qb, &filter);
    qb.push(" ORDER BY s.last_seen ")
        .push(order.sql())
        .push(" LIMIT ")
        .push_bind(limit_to_i64(limit))
        .push(" OFFSET ")
        .push_bind(limit_to_i64(offset))
        .push("\n)\n")
        .push(LISTING_TAIL_SQL)
        .push("\nORDER BY ranked.last_seen ")
        .push(order.sql());
    let rows = qb.build_query_as().fetch_all(pool).await?;
    Ok(rows)
}

pub(super) fn listing_by_ids_query(ids: &[uuid::Uuid]) -> QueryBuilder<Postgres> {
    let mut qb = QueryBuilder::<Postgres>::new(RANKED_HEAD_SQL);
    qb.push(" WHERE s.session_uuid = ANY(")
        .push_bind(ids)
        .push(")\n)\n")
        .push(LISTING_TAIL_SQL)
        .push("\nORDER BY ranked.last_seen DESC");
    qb
}

pub async fn list_sessions_paginated(
    pool: &PgPool,
    offset: usize,
    limit: usize,
    order: SortOrder,
    date_from: Option<DateTime<Utc>>,
    date_to: Option<DateTime<Utc>>,
) -> Result<Vec<SessionSummaryRow>, PersistError> {
    let filter = SessionPageFilter {
        date_from,
        date_to,
        ..SessionPageFilter::default()
    };
    list_sessions_page(pool, filter, offset, limit, order).await
}
