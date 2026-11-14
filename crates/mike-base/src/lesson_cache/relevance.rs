use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use super::marks::{ChangeMark, LessonOutcome};
use super::outcome;
use super::snapshot::{InjectableLesson, LessonSnapshot};

#[cfg(test)]
#[path = "relevance_tests.rs"]
mod relevance_tests;

const QUERY_CAPACITY: usize = 128;
const SESSION_CAPACITY: usize = 1024;
const MAX_QUERY_BYTES: usize = 8192;
const ABSOLUTE_FLOOR: f32 = 0.05;
const RELATIVE_FLOOR: f32 = 0.5;
const RESEND_AFTER_REQUESTS: u64 = 40;

#[derive(Debug, Default)]
pub struct LessonRelevance {
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    model: String,
    pending: VecDeque<(String, String, String)>,
    queries: HashMap<String, Vec<f32>>,
    documents: HashMap<i64, Vec<f32>>,
    delivered: HashMap<String, HashMap<String, u64>>,
    requests: HashMap<String, u64>,
    mod_sessions: HashSet<String>,
    vectors_key: Option<VectorsKey>,
    outcomes: outcome::Outcomes,
}

#[derive(Debug, PartialEq, Eq)]
pub struct VectorsKey {
    pub model_name: String,
    pub fingerprint: ChangeMark,
    pub eligible: HashSet<i64>,
}

impl LessonRelevance {
    pub fn forget_session(&self, session: &str) {
        if let Ok(mut state) = self.state.try_lock() {
            state.delivered.remove(session);
            state.requests.remove(session);
        }
    }

    pub fn note_mod_session(&self, session: &str) {
        let Ok(mut state) = self.state.try_lock() else {
            return;
        };
        if state.mod_sessions.len() >= SESSION_CAPACITY * 4 {
            state.mod_sessions.clear();
        }
        state.mod_sessions.insert(session.to_owned());
    }

    pub fn is_mod_session(&self, session: &str) -> bool {
        self.state
            .try_lock()
            .is_ok_and(|state| state.mod_sessions.contains(session))
    }

    pub fn select<'a>(
        &self,
        lessons: &[&'a InjectableLesson],
        session: &str,
        query: &str,
        limit: usize,
    ) -> Vec<&'a InjectableLesson> {
        let key = fingerprint(query);
        let mut state = self.state.try_lock().ok();
        if let Some(state) = state.as_mut() {
            queue_query(state, session, &key, query);
        }
        let terms = terms(query);
        let weights = query_weights(&terms, lessons);
        let mut ranked: Vec<_> = lessons
            .iter()
            .copied()
            .filter(|lesson| lesson.agent_id.is_none())
            .filter(|lesson| !already_delivered(state.as_deref(), session, lesson))
            .map(|lesson| {
                (
                    lesson,
                    score(state.as_deref(), session, &key, &weights, lesson),
                )
            })
            .collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        let floor = supply_floor(query, &ranked);
        let mut remaining = 6000usize;
        let chosen: Vec<_> = ranked
            .into_iter()
            .take_while(|(_, relevance)| *relevance >= floor)
            .filter(|(lesson, _)| {
                if lesson.text.len() > remaining {
                    return false;
                }
                remaining -= lesson.text.len();
                true
            })
            .take(limit)
            .collect();
        super::scores::remember_scores(session, &chosen);
        chosen.into_iter().map(|(lesson, _)| lesson).collect()
    }

    pub fn observe_request(&self, session: &str) {
        let Ok(mut state) = self.state.try_lock() else {
            return;
        };
        if state.requests.len() >= SESSION_CAPACITY
            && !state.requests.contains_key(session)
            && let Some(oldest) = state.requests.keys().next().cloned()
        {
            state.requests.remove(&oldest);
            state.delivered.remove(&oldest);
        }
        *state.requests.entry(session.to_owned()).or_default() += 1;
    }

    pub fn query_ready(&self, session: &str, query: &str) -> bool {
        let Ok(mut state) = self.state.try_lock() else {
            return false;
        };
        let key = fingerprint(query);
        queue_query(&mut state, session, &key, query);
        state.queries.contains_key(&key)
    }

    pub fn delivered(&self, session: &str, lessons: &[&InjectableLesson]) {
        let Ok(mut state) = self.state.try_lock() else {
            return;
        };
        if state.delivered.len() >= SESSION_CAPACITY
            && !state.delivered.contains_key(session)
            && let Some(key) = state.delivered.keys().next().cloned()
        {
            state.delivered.remove(&key);
        }
        let now = state.requests.get(session).copied().unwrap_or_default();
        let delivered = state.delivered.entry(session.to_owned()).or_default();
        if delivered.len() >= 256 {
            delivered.retain(|_, at| now.saturating_sub(*at) < RESEND_AFTER_REQUESTS);
        }
        delivered.extend(lessons.iter().map(|lesson| (lesson_key(lesson), now)));
    }

    pub async fn replace_outcomes(&self, rows: Vec<LessonOutcome>) {
        self.state
            .lock()
            .await
            .outcomes
            .replace(rows, Instant::now());
    }

    pub async fn outcomes_due(&self) -> bool {
        self.state.lock().await.outcomes.due(Instant::now())
    }

    pub async fn touch_outcomes(&self) {
        self.state.lock().await.outcomes.touch(Instant::now());
    }

    pub async fn holds_vectors(&self, key: &VectorsKey) -> bool {
        self.state.lock().await.vectors_key.as_ref() == Some(key)
    }

    pub async fn forget_vectors(&self) {
        let mut state = self.state.lock().await;
        state.documents.clear();
        state.queries.clear();
        state.vectors_key = None;
    }

    pub async fn take_pending(
        &self,
        model: &str,
        changed: Option<(VectorsKey, HashMap<i64, Vec<f32>>)>,
    ) -> Vec<(String, String, String)> {
        let mut state = self.state.lock().await;
        if state.model != model {
            state.model = model.to_owned();
            state.queries.clear();
        }
        if let Some((key, documents)) = changed {
            state.documents = documents;
            state.vectors_key = Some(key);
        }
        let count = state.pending.len().min(8);
        state.pending.drain(..count).collect()
    }

    pub async fn cache_query(
        &self,
        current_model: &str,
        model: &str,
        key: String,
        vector: Vec<f32>,
    ) -> bool {
        let mut state = self.state.lock().await;
        if current_model != model {
            return false;
        }
        if state.queries.len() >= QUERY_CAPACITY
            && let Some(oldest) = state.queries.keys().next().cloned()
        {
            state.queries.remove(&oldest);
        }
        state.queries.insert(key, vector);
        true
    }
}

fn queue_query(state: &mut State, session: &str, key: &str, query: &str) {
    if query.is_empty()
        || state.queries.contains_key(key)
        || state.pending.iter().any(|(_, pending, _)| pending == key)
        || state.pending.len() >= QUERY_CAPACITY
    {
        return;
    }
    let mut chunks: Vec<_> = crate::analysis::embeddings::chunk_text(query, MAX_QUERY_BYTES / 8)
        .into_iter()
        .rev()
        .take(8)
        .collect();
    chunks.reverse();
    state
        .pending
        .push_back((session.to_owned(), key.to_owned(), chunks.join("\n")));
}

fn already_delivered(state: Option<&State>, session: &str, lesson: &InjectableLesson) -> bool {
    let Some(state) = state else {
        return false;
    };
    let now = state.requests.get(session).copied().unwrap_or_default();
    state
        .delivered
        .get(session)
        .and_then(|keys| keys.get(&lesson_key(lesson)))
        .is_some_and(|at| now.saturating_sub(*at) < RESEND_AFTER_REQUESTS)
}

fn supply_floor(query: &str, ranked: &[(&InjectableLesson, f32)]) -> f32 {
    if query.is_empty() {
        return f32::NEG_INFINITY;
    }
    let best = ranked.first().map_or(0.0, |(_, relevance)| *relevance);
    ABSOLUTE_FLOOR.max(best * RELATIVE_FLOOR)
}

fn lesson_key(lesson: &InjectableLesson) -> String {
    format!(
        "{}:{}",
        lesson.id,
        fingerprint(&format!(
            "{}:{}:{:?}",
            lesson.kind, lesson.text, lesson.topic
        ))
    )
}

fn fingerprint(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn terms(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() > 2)
        .map(str::to_lowercase)
        .filter(|word| {
            !matches!(
                word.as_str(),
                "the" | "and" | "for" | "that" | "this" | "with" | "you" | "are"
            )
        })
        .collect()
}

fn query_weights(query: &HashSet<String>, lessons: &[&InjectableLesson]) -> HashMap<String, f32> {
    let documents: Vec<_> = lessons.iter().map(|lesson| terms(&lesson.text)).collect();
    query
        .iter()
        .map(|word| {
            let frequency = documents
                .iter()
                .filter(|words| words.contains(word))
                .count();
            let weight = ((documents.len() as f32 + 1.0) / (frequency as f32 + 1.0)).ln() + 0.1;
            (word.clone(), weight)
        })
        .collect()
}

fn score(
    state: Option<&State>,
    _session: &str,
    key: &str,
    query: &HashMap<String, f32>,
    lesson: &InjectableLesson,
) -> f32 {
    let words = terms(&lesson.text);
    let matching: f32 = query
        .iter()
        .filter(|(word, _)| words.contains(*word))
        .map(|(_, weight)| weight)
        .sum();
    let total: f32 = query.values().sum();
    let lexical = matching / total.max(1.0) / (1.0 + words.len() as f32 / 40.0);
    let semantic = state
        .and_then(|s| Some((s.queries.get(key)?, s.documents.get(&lesson.id)?)))
        .map(|(query, document)| cosine(query, document))
        .unwrap_or(0.0);
    let matched = lexical + if semantic >= 0.55 { semantic } else { 0.0 };
    matched * state.map_or(1.0, |s| s.outcomes.weight(lesson.id))
}

pub fn eligible_ids(snapshot: &LessonSnapshot) -> HashSet<i64> {
    snapshot
        .by_project
        .values()
        .flatten()
        .chain(snapshot.shared.iter())
        .map(|l| l.id)
        .collect()
}

fn cosine(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() {
        return 0.0;
    }
    let dot: f32 = left.iter().zip(right).map(|(a, b)| a * b).sum();
    let norm =
        (left.iter().map(|v| v * v).sum::<f32>() * right.iter().map(|v| v * v).sum::<f32>()).sqrt();
    if norm > 0.0 { dot / norm } else { 0.0 }
}
