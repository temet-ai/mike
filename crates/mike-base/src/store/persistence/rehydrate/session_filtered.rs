use sqlx::PgPool;

use super::pagination::PageWindow;
use super::session_aggregates::SessionSummaryRow;
use super::session_summary::{SessionPageFilter, list_sessions_page};
use crate::store::persistence::PersistError;

pub struct SessionListFilterParams<'a> {
    pub window: PageWindow,
    pub search: Option<&'a str>,
    pub min_request_count: Option<i64>,
    pub project: Option<&'a str>,
}

pub async fn list_sessions_paginated_filtered(
    pool: &PgPool,
    params: SessionListFilterParams<'_>,
) -> Result<Vec<SessionSummaryRow>, PersistError> {
    let window = params.window;
    let filter = SessionPageFilter {
        date_from: window.date_from,
        date_to: window.date_to,
        search: params.search.map(str::trim).filter(|s| !s.is_empty()),
        project: params.project.map(str::trim).filter(|s| !s.is_empty()),
        min_request_count: params.min_request_count,
    };
    list_sessions_page(pool, filter, window.offset, window.limit, window.order).await
}
