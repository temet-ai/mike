use chrono::{DateTime, Utc};
use sqlx::PgPool;

use super::PersistError;

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct ContentSearchTotals {
    pub(crate) total_matches: i64,
    pub(crate) total_sessions: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct ContentSearchRow {
    pub(crate) session_id: String,
    pub(crate) project_path: String,
    pub(crate) project_display_name: String,
    pub(crate) title: Option<String>,
    pub(crate) match_count: i64,
    pub(crate) turn_count: i64,
    pub(crate) last_match_at: DateTime<Utc>,
    pub(crate) turn_id: i64,
    pub(crate) request_id: i64,
    pub(crate) role: String,
    pub(crate) chunk_idx: i32,
    pub(crate) chunk_text: String,
    pub(crate) created_at: DateTime<Utc>,
}

pub(crate) async fn search_content_totals(
    pool: &PgPool,
    query: &str,
    project_path: Option<&str>,
) -> Result<ContentSearchTotals, PersistError> {
    let totals = match project_path {
        Some(project_path) => {
            sqlx::query_file_as!(
                ContentSearchTotals,
                "src/store/persistence/content_search/totals_in_project.sql",
                query,
                project_path,
            )
            .fetch_one(pool)
            .await?
        }
        None => {
            sqlx::query_file_as!(
                ContentSearchTotals,
                "src/store/persistence/content_search/totals.sql",
                query,
            )
            .fetch_one(pool)
            .await?
        }
    };
    Ok(totals)
}

pub(crate) async fn search_content_page(
    pool: &PgPool,
    query: &str,
    project_path: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ContentSearchRow>, PersistError> {
    let rows = match project_path {
        Some(project_path) => {
            sqlx::query_file_as!(
                ContentSearchRow,
                "src/store/persistence/content_search/page_in_project.sql",
                query,
                project_path,
                limit,
                offset,
            )
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_file_as!(
                ContentSearchRow,
                "src/store/persistence/content_search/page.sql",
                query,
                limit,
                offset,
            )
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows)
}
