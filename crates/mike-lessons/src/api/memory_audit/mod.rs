pub mod archive;
pub mod consolidation;
pub mod outcomes;
pub mod relations;
pub mod session_lessons;

pub use archive::{list_archived_lessons, restore_lesson};
pub use consolidation::{
    get_consolidation_run, get_lesson_history, list_consolidation_decisions,
    list_consolidation_runs, list_session_memory_actions,
};
pub use outcomes::list_lesson_outcomes;
pub use relations::{
    confirm_lesson_relation, list_lesson_relations, list_lesson_relations_for_lesson,
    reject_lesson_relation,
};
pub use session_lessons::list_session_lessons;

use serde::Serialize;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 500;

#[derive(Debug, Serialize)]
pub struct Paged<T> {
    pub items: Vec<T>,
    pub total_count: i64,
    pub limit: i64,
    pub offset: i64,
}

fn clamp_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

fn clamp_offset(offset: Option<i64>) -> i64 {
    offset.unwrap_or(0).max(0)
}

fn parse_session_filter(session: Option<&str>) -> Result<Option<uuid::Uuid>, &'static str> {
    match session {
        None => Ok(None),
        Some(s) => uuid::Uuid::parse_str(s)
            .map(Some)
            .map_err(|_| "`session` must be a UUID"),
    }
}
