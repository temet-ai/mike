use std::collections::HashMap;
use std::sync::LazyLock;

use dashmap::DashMap;

use super::snapshot::InjectableLesson;

const SESSION_CAPACITY: usize = 4096;
const SCORES_PER_SESSION: usize = 256;

static SCORES: LazyLock<DashMap<String, HashMap<i64, f32>>> = LazyLock::new(DashMap::new);

pub fn remember_scores(session: &str, chosen: &[(&InjectableLesson, f32)]) {
    if chosen.is_empty() {
        return;
    }
    if SCORES.len() >= SESSION_CAPACITY && !SCORES.contains_key(session) {
        SCORES.clear();
    }
    let mut scores = SCORES.entry(session.to_owned()).or_default();
    if scores.len() >= SCORES_PER_SESSION {
        scores.clear();
    }
    scores.extend(chosen.iter().map(|(lesson, score)| (lesson.id, *score)));
}

pub fn chosen_score(session: &str, lesson_id: i64) -> Option<f64> {
    SCORES
        .get(session)?
        .get(&lesson_id)
        .map(|score| f64::from(*score))
}
