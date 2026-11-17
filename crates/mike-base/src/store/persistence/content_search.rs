use chrono::{DateTime, Utc};
use sqlx::PgPool;

use super::PersistError;

#[cfg(test)]
#[path = "content_search_tests.rs"]
mod content_search_tests;

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

pub const MAX_QUERY_WORDS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Phrase,
    All,
    Any,
}

impl SearchMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "phrase" => Some(Self::Phrase),
            "all" => Some(Self::All),
            "any" => Some(Self::Any),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Phrase => "phrase",
            Self::All => "all",
            Self::Any => "any",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContentQueryError {
    #[error("q has no words")]
    NoWords,
    #[error("q has {count} words, at most {max} are allowed")]
    TooManyWords { count: usize, max: usize },
}

#[derive(Debug, Clone)]
pub struct ContentQuery {
    mode: SearchMode,
    words: Vec<String>,
    patterns: Vec<String>,
    min_words: i32,
}

impl ContentQuery {
    pub fn phrase(q: &str) -> Self {
        Self {
            mode: SearchMode::Phrase,
            words: vec![q.to_owned()],
            patterns: vec![like_pattern(q)],
            min_words: 1,
        }
    }

    pub fn new(q: &str, mode: SearchMode) -> Result<Self, ContentQueryError> {
        if mode == SearchMode::Phrase {
            return Ok(Self::phrase(q));
        }
        let count = q.split_whitespace().count();
        if count == 0 {
            return Err(ContentQueryError::NoWords);
        }
        if count > MAX_QUERY_WORDS {
            return Err(ContentQueryError::TooManyWords {
                count,
                max: MAX_QUERY_WORDS,
            });
        }
        let words = distinct_words(q);
        let min_words = if mode == SearchMode::All {
            words.len() as i32
        } else {
            1
        };
        let patterns = words.iter().map(|w| like_pattern(w)).collect();
        Ok(Self {
            mode,
            words,
            patterns,
            min_words,
        })
    }

    pub fn mode(&self) -> SearchMode {
        self.mode
    }

    pub fn words(&self) -> &[String] {
        &self.words
    }
}

fn like_pattern(word: &str) -> String {
    format!("%{word}%")
}

fn distinct_words(q: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    q.split_whitespace()
        .filter(|w| seen.insert(w.to_lowercase()))
        .map(str::to_owned)
        .collect()
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
    pub words_matched: i64,
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
    query: &ContentQuery,
    project_id: Option<i64>,
) -> Result<ContentSearchTotals, PersistError> {
    let totals = match project_id {
        Some(project_id) => {
            sqlx::query_file_as!(
                ContentSearchTotals,
                "src/store/persistence/content_search/totals_in_project.sql",
                &query.patterns,
                query.min_words,
                project_id,
            )
            .fetch_one(pool)
            .await?
        }
        None => {
            sqlx::query_file_as!(
                ContentSearchTotals,
                "src/store/persistence/content_search/totals.sql",
                &query.patterns,
                query.min_words,
            )
            .fetch_one(pool)
            .await?
        }
    };
    Ok(totals)
}

pub async fn search_content_page(
    pool: &PgPool,
    query: &ContentQuery,
    project_id: Option<i64>,
    page: &PageRequest,
) -> Result<Vec<ContentSearchRow>, PersistError> {
    let order = page.order.as_str();
    let rows = match project_id {
        Some(project_id) => {
            sqlx::query_file_as!(
                ContentSearchRow,
                "src/store/persistence/content_search/page_in_project.sql",
                &query.patterns,
                query.min_words,
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
                &query.patterns,
                query.min_words,
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
