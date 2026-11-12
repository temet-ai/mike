use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::store::persistence::lessons::LessonOutcome;

const RELOAD_EVERY: Duration = Duration::from_secs(600);

#[derive(Debug, Default)]
pub(super) struct Outcomes {
    by_lesson: HashMap<i64, LessonOutcome>,
    loaded_at: Option<Instant>,
}

impl Outcomes {
    pub(super) fn due(&self, now: Instant) -> bool {
        self.loaded_at
            .is_none_or(|at| now.duration_since(at) >= RELOAD_EVERY)
    }

    pub(super) fn replace(&mut self, rows: Vec<LessonOutcome>, now: Instant) {
        self.by_lesson = rows.into_iter().map(|row| (row.lesson_id, row)).collect();
        self.loaded_at = Some(now);
    }

    pub(super) fn touch(&mut self, now: Instant) {
        self.loaded_at = Some(now);
    }

    pub(super) fn weight(&self, lesson_id: i64) -> f32 {
        self.by_lesson.get(&lesson_id).map_or(1.0, factor)
    }
}

pub(super) fn factor(outcome: &LessonOutcome) -> f32 {
    2.0 * (outcome.used as f32 + 1.0) / (outcome.judged as f32 + 2.0)
}
