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
const TRIGRAM_CHARS: usize = 3;

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
    scan_all: bool,
}

impl ContentQuery {
    pub fn phrase(q: &str) -> Self {
        Self::from_words(SearchMode::Phrase, vec![q.to_owned()], 1)
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
        Ok(Self::from_words(mode, words, min_words))
    }

    fn from_words(mode: SearchMode, words: Vec<String>, min_words: i32) -> Self {
        Self {
            mode,
            patterns: words.iter().map(|w| like_pattern(w)).collect(),
            min_words,
            scan_all: words.iter().any(|w| w.chars().count() < TRIGRAM_CHARS),
            words,
        }
    }

    pub fn mode(&self) -> SearchMode {
        self.mode
    }

    pub fn words(&self) -> &[String] {
        &self.words
    }

    pub fn matches_are_literal(&self) -> bool {
        self.words
            .iter()
            .all(|w| w.is_ascii() && !w.contains(['%', '_', '\\']))
    }
}

fn like_pattern(word: &str) -> String {
    let folded: String = word
        .chars()
        .filter_map(|c| c.to_lowercase().next())
        .collect();
    format!("%{folded}%")
}

fn distinct_words(q: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    q.split_whitespace()
        .filter(|w| seen.insert(w.to_lowercase()))
        .map(str::to_owned)
        .collect()
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ContentSearchTotals {
    pub total_matches: i64,
    pub total_sessions: i64,
}

#[derive(Debug)]
pub struct ContentSearchPage {
    pub totals: ContentSearchTotals,
    pub rows: Vec<ContentSearchRow>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct ContentSearchRow {
    pub total_matches: i64,
    pub total_sessions: i64,
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

#[derive(Debug, Clone, Copy)]
pub struct PageRequest {
    pub order: SessionOrder,
    pub limit: i64,
    pub offset: i64,
    pub matches_per_session: Option<i64>,
}

pub async fn search_content(
    pool: &PgPool,
    query: &ContentQuery,
    project_id: Option<i64>,
    page: &PageRequest,
) -> Result<ContentSearchPage, PersistError> {
    let rows = search_content_rows(pool, query, project_id, page).await?;
    if rows.is_empty() && page.offset > 0 {
        let first = PageRequest {
            limit: 1,
            offset: 0,
            matches_per_session: Some(1),
            ..*page
        };
        let counted = search_content_rows(pool, query, project_id, &first).await?;
        return Ok(ContentSearchPage {
            totals: totals_of(&counted),
            rows,
        });
    }
    Ok(ContentSearchPage {
        totals: totals_of(&rows),
        rows,
    })
}

fn totals_of(rows: &[ContentSearchRow]) -> ContentSearchTotals {
    rows.first()
        .map(|row| ContentSearchTotals {
            total_matches: row.total_matches,
            total_sessions: row.total_sessions,
        })
        .unwrap_or_default()
}

async fn search_content_rows(
    pool: &PgPool,
    query: &ContentQuery,
    project_id: Option<i64>,
    page: &PageRequest,
) -> Result<Vec<ContentSearchRow>, PersistError> {
    let rows = sqlx::query_file_as!(
        ContentSearchRow,
        "src/store/persistence/content_search/page.sql",
        &query.patterns,
        query.min_words,
        query.scan_all,
        project_id,
        page.limit,
        page.offset,
        page.order.as_str(),
        page.matches_per_session,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
