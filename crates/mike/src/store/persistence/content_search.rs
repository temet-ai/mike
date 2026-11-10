use chrono::{DateTime, Utc};
use sqlx::PgPool;

use super::PersistError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionOrder {
    Matches,
    Recent,
}

impl SessionOrder {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "matches" => Some(Self::Matches),
            "recent" => Some(Self::Recent),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Matches => "matches",
            Self::Recent => "recent",
        }
    }
}

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
    pub(crate) prompt_match_count: i64,
    pub(crate) turn_count: i64,
    pub(crate) last_match_at: DateTime<Utc>,
    pub(crate) turn_id: i64,
    pub(crate) request_id: i64,
    pub(crate) role: String,
    pub(crate) chunk_idx: i32,
    pub(crate) chunk_text: String,
    pub(crate) created_at: DateTime<Utc>,
}

pub(crate) struct PageRequest {
    pub(crate) order: SessionOrder,
    pub(crate) limit: i64,
    pub(crate) offset: i64,
}

pub(crate) async fn search_content_totals(
    pool: &PgPool,
    query: &str,
    project_id: Option<i64>,
) -> Result<ContentSearchTotals, PersistError> {
    let totals = match project_id {
        Some(project_id) => {
            sqlx::query_file_as!(
                ContentSearchTotals,
                "src/store/persistence/content_search/totals_in_project.sql",
                query,
                project_id,
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
    project_id: Option<i64>,
    page: &PageRequest,
) -> Result<Vec<ContentSearchRow>, PersistError> {
    let order = page.order.as_str();
    let rows = match project_id {
        Some(project_id) => {
            sqlx::query_file_as!(
                ContentSearchRow,
                "src/store/persistence/content_search/page_in_project.sql",
                query,
                project_id,
                page.limit,
                page.offset,
                order,
            )
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_file_as!(
                ContentSearchRow,
                "src/store/persistence/content_search/page.sql",
                query,
                page.limit,
                page.offset,
                order,
            )
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows)
}
