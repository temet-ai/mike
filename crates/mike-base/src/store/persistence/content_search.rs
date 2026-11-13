use chrono::{DateTime, Utc};
use sqlx::PgPool;

use super::PersistError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionOrder {
    Matches,
    Recent,
}

impl SessionOrder {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "matches" => Some(Self::Matches),
            "recent" => Some(Self::Recent),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Matches => "matches",
            Self::Recent => "recent",
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct ContentSearchTotals {
    pub total_matches: i64,
    pub total_sessions: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub struct ContentSearchRow {
    pub session_id: String,
    pub project_path: String,
    pub project_display_name: String,
    pub title: Option<String>,
    pub match_count: i64,
    pub prompt_match_count: i64,
    pub turn_count: i64,
    pub last_match_at: DateTime<Utc>,
    pub turn_id: i64,
    pub request_id: i64,
    pub role: String,
    pub chunk_idx: i32,
    pub chunk_text: String,
    pub created_at: DateTime<Utc>,
}

pub struct PageRequest {
    pub order: SessionOrder,
    pub limit: i64,
    pub offset: i64,
}

pub async fn search_content_totals(
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

pub async fn search_content_page(
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
