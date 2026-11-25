use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::PersistError;

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct RecordTurnRow {
    pub turn_id: i64,
    pub opened_at: DateTime<Utc>,
    pub opened_by: Option<String>,
    pub prompt_kind: Option<String>,
    pub prompt: String,
    pub stop_text: Option<String>,
    pub last_text: Option<String>,
    pub stop_reason: Option<String>,
    pub aborted: Option<bool>,
    pub interrupted: Option<bool>,
    pub ended_on: Option<String>,
    pub checks_count: Option<i32>,
    pub checks_failed: Option<i32>,
    pub files_touched: i32,
    pub tool_names: Vec<String>,
    pub tool_counts: Vec<i32>,
    pub tool_errors: i32,
    pub commit_count: i32,
    pub commit_commands: Vec<String>,
    pub push_count: i32,
    pub push_commands: Vec<String>,
}

impl RecordTurnRow {
    pub fn tools(&self) -> impl Iterator<Item = (&str, i32)> {
        self.tool_names
            .iter()
            .map(String::as_str)
            .zip(self.tool_counts.iter().copied())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct RecapRow {
    pub request_id: i64,
    pub at: DateTime<Utc>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct SuppliedLessonRow {
    pub lesson_id: i64,
    pub text: String,
    pub route: String,
    pub request_id: i64,
    pub turn_id: Option<i64>,
    pub supplied_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SessionRecord {
    pub turns: Vec<RecordTurnRow>,
    pub recaps: Vec<RecapRow>,
    pub lessons: Vec<SuppliedLessonRow>,
}

impl SessionRecord {
    pub fn is_empty(&self) -> bool {
        self.turns.is_empty() && self.recaps.is_empty() && self.lessons.is_empty()
    }
}

const TURNS: &str = include_str!("record_turns.sql");
const RECAPS: &str = include_str!("record_recaps.sql");
const LESSONS: &str = include_str!("record_lessons.sql");

impl crate::store::persistence::dossier_repo::DossierDb<'_> {
    pub async fn fetch_session_record(
        self,
        session_db_id: i64,
        at: DateTime<Utc>,
    ) -> Result<SessionRecord, PersistError> {
        crate::metrics::time_db("fetch_session_record", async move {
            let turns = sqlx::query_as::<_, RecordTurnRow>(TURNS)
                .bind(session_db_id)
                .bind(at)
                .fetch_all(&self.pool);
            let recaps = sqlx::query_as::<_, RecapRow>(RECAPS)
                .bind(session_db_id)
                .bind(at)
                .fetch_all(&self.pool);
            let lessons = sqlx::query_as::<_, SuppliedLessonRow>(LESSONS)
                .bind(session_db_id)
                .bind(at)
                .fetch_all(&self.pool);
            let (turns, recaps, mut lessons) = tokio::try_join!(turns, recaps, lessons)?;
            lessons.reverse();
            Ok(SessionRecord {
                turns,
                recaps,
                lessons,
            })
        })
        .await
    }
}
