use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChangeMark {
    pub generation: i64,
    pub expired: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, sqlx::FromRow)]
pub struct LessonOutcome {
    pub lesson_id: i64,
    pub judged: i64,
    pub used: i64,
    pub corrected_anyway: i64,
    pub ignored: i64,
}
