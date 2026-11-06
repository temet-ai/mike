use sqlx::PgPool;

use super::pagination::PageWindow;
use super::session_aggregates::SessionSummaryRow;
use super::session_summary::{SessionPageFilter, list_sessions_page};
use crate::store::persistence::PersistError;

pub async fn list_sessions_paginated_with_search(
    pool: &PgPool,
    window: PageWindow,
    search: Option<&str>,
) -> Result<Vec<SessionSummaryRow>, PersistError> {
    let filter = SessionPageFilter {
        date_from: window.date_from,
        date_to: window.date_to,
        search: search.map(str::trim).filter(|s| !s.is_empty()),
        ..SessionPageFilter::default()
    };
    list_sessions_page(pool, filter, window.offset, window.limit, window.order).await
}
