use std::collections::{HashMap, HashSet, VecDeque};

use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use super::{InjectableLesson, LessonSnapshot};
use crate::analysis::EmbeddingService;
use crate::store::persistence::lessons::ChangeMark;
use crate::store::persistence::{PersistError, PersistHandle};

const QUERY_CAPACITY: usize = 128;
const SESSION_CAPACITY: usize = 1024;
const MAX_QUERY_BYTES: usize = 8192;

#[derive(Debug, Default)]
pub(crate) struct LessonRelevance {
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    model: String,
    pending: VecDeque<(String, String, String)>,
    queries: HashMap<String, Vec<f32>>,
    documents: HashMap<i64, Vec<f32>>,
    delivered: HashMap<String, HashSet<String>>,
    context_sizes: HashMap<String, usize>,
    mod_sessions: HashSet<String>,
    vectors_key: Option<VectorsKey>,
}

#[derive(Debug, PartialEq, Eq)]
struct VectorsKey {
    model_name: String,
    fingerprint: ChangeMark,
    eligible: HashSet<i64>,
}

impl LessonRelevance {
    pub(crate) fn forget_session(&self, session: &str) {
        if let Ok(mut state) = self.state.try_lock() {
            state.delivered.remove(session);
            state.context_sizes.remove(session);
        }
    }

    pub(crate) fn note_mod_session(&self, session: &str) {
        let Ok(mut state) = self.state.try_lock() else {
            return;
        };
        if state.mod_sessions.len() >= SESSION_CAPACITY * 4 {
            state.mod_sessions.clear();
        }
        state.mod_sessions.insert(session.to_owned());
    }

    pub(crate) fn is_mod_session(&self, session: &str) -> bool {
        self.state
            .try_lock()
            .is_ok_and(|state| state.mod_sessions.contains(session))
    }

    pub(crate) fn select<'a>(
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
            .filter(|(_, relevance)| query.is_empty() || *relevance > 0.0)
            .collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut remaining = 6000usize;
        ranked
            .into_iter()
            .filter_map(|(lesson, _)| {
                if lesson.text.len() > remaining {
                    return None;
                }
                remaining -= lesson.text.len();
                Some(lesson)
            })
            .take(limit)
            .collect()
    }

    pub(crate) fn observe_context(&self, session: &str, messages: usize) {
        let Ok(mut state) = self.state.try_lock() else {
            return;
        };
        if state.context_sizes.len() >= SESSION_CAPACITY
            && !state.context_sizes.contains_key(session)
            && let Some(oldest) = state.context_sizes.keys().next().cloned()
        {
            state.context_sizes.remove(&oldest);
            state.delivered.remove(&oldest);
        }
        if state
            .context_sizes
            .insert(session.to_owned(), messages)
            .is_some_and(|previous| messages < previous)
        {
            state.delivered.remove(session);
        }
    }

    pub(crate) fn query_ready(&self, session: &str, query: &str) -> bool {
        let Ok(mut state) = self.state.try_lock() else {
            return false;
        };
        let key = fingerprint(query);
        queue_query(&mut state, session, &key, query);
        state.queries.contains_key(&key)
    }

    pub(crate) fn delivered(&self, session: &str, lessons: &[&InjectableLesson]) {
        let Ok(mut state) = self.state.try_lock() else {
            return;
        };
        if state.delivered.len() >= SESSION_CAPACITY
            && !state.delivered.contains_key(session)
            && let Some(key) = state.delivered.keys().next().cloned()
        {
            state.delivered.remove(&key);
        }
        let delivered = state.delivered.entry(session.to_owned()).or_default();
        if delivered.len() >= 256 {
            delivered.clear();
        }
        delivered.extend(lessons.iter().map(|lesson| lesson_key(lesson)));
    }

    pub(crate) async fn refresh(
        &self,
        service: &EmbeddingService,
        persist: &PersistHandle,
        snapshot: &LessonSnapshot,
    ) {
        if !service.is_available() {
            let mut state = self.state.lock().await;
            state.documents.clear();
            state.queries.clear();
            state.vectors_key = None;
            return;
        }
        let model = format!("{}:{}", service.url(), service.model_name());
        let eligible: HashSet<_> = snapshot
            .by_project
            .values()
            .flatten()
            .chain(snapshot.shared.iter())
            .map(|l| l.id)
            .collect();
        let changed = match self
            .load_changed_vectors(persist, &service.model_name(), eligible)
            .await
        {
            Ok(changed) => changed,
            Err(error) => {
                tracing::warn!(%error, "lesson relevance vectors unavailable; using lexical ranking");
                return;
            }
        };
        let pending = {
            let mut state = self.state.lock().await;
            if state.model != model {
                state.model = model.clone();
                state.queries.clear();
            }
            if let Some((key, documents)) = changed {
                state.documents = documents;
                state.vectors_key = Some(key);
            }
            let count = state.pending.len().min(8);
            state.pending.drain(..count).collect::<Vec<_>>()
        };
        self.embed_pending_queries(service, &model, pending).await;
    }

    async fn load_changed_vectors(
        &self,
        persist: &PersistHandle,
        model_name: &str,
        eligible: HashSet<i64>,
    ) -> Result<Option<(VectorsKey, HashMap<i64, Vec<f32>>)>, PersistError> {
        let fingerprint = persist.injectable_lesson_vectors_fingerprint().await?;
        let key = VectorsKey {
            model_name: model_name.to_owned(),
            fingerprint,
            eligible,
        };
        if self.state.lock().await.vectors_key.as_ref() == Some(&key) {
            return Ok(None);
        }
        let documents = persist
            .injectable_lesson_vectors(model_name)
            .await?
            .into_iter()
            .filter(|(id, _)| key.eligible.contains(id))
            .collect();
        Ok(Some((key, documents)))
    }

    async fn embed_pending_queries(
        &self,
        service: &EmbeddingService,
        model: &str,
        pending: Vec<(String, String, String)>,
    ) {
        for (_, key, query) in pending {
            match service.embed_query(&query).await {
                Ok(vector) => {
                    if !self.cache_query(service, model, key, vector).await {
                        return;
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "lesson query embedding failed; lexical ranking remains available")
                }
            }
        }
    }

    async fn cache_query(
        &self,
        service: &EmbeddingService,
        model: &str,
        key: String,
        vector: Vec<f32>,
    ) -> bool {
        let mut state = self.state.lock().await;
        if format!("{}:{}", service.url(), service.model_name()) != model {
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
    state
        .and_then(|s| s.delivered.get(session))
        .is_some_and(|ids| ids.contains(&lesson_key(lesson)))
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
    lexical + if semantic >= 0.55 { semantic } else { 0.0 }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinctive_task_terms_outweigh_repeated_project_boilerplate() {
        let snapshot = LessonSnapshot::from_pairs([(
            "/repo".into(),
            vec![
                ("convention".into(), "project checks alpha".into()),
                ("convention".into(), "project checks beta".into()),
                ("convention".into(), "project checks gamma".into()),
                ("convention".into(), "project checks delta".into()),
                ("correction".into(), "cache invalidation".into()),
            ],
        )]);
        let candidates: Vec<_> = snapshot.for_project("/repo").iter().collect();
        let selected = snapshot
            .relevance
            .select(&candidates, "session", "project checks cache", 1);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].text, "cache invalidation");
    }

    #[test]
    fn lexical_matching_beats_recency_and_delivery_is_versioned() {
        let snapshot = LessonSnapshot::from_pairs([(
            "project".into(),
            vec![
                ("correction".into(), "Use blue backgrounds".into()),
                (
                    "correction".into(),
                    "Reuse cargo test results before running cargo again".into(),
                ),
            ],
        )]);
        let cache = LessonRelevance::default();
        let selected = cache.select(
            &snapshot.for_project("project").iter().collect::<Vec<_>>(),
            "session",
            "cargo test",
            1,
        );
        assert!(selected[0].text.contains("cargo"));
        cache.delivered("session", &selected);
        let next = cache.select(
            &snapshot.for_project("project").iter().collect::<Vec<_>>(),
            "session",
            "cargo test",
            1,
        );
        assert!(next.is_empty());
        assert_eq!(
            cache.select(
                &snapshot.for_project("project").iter().collect::<Vec<_>>(),
                "other",
                "cargo test",
                1
            )[0]
            .id,
            selected[0].id
        );
    }

    #[tokio::test]
    async fn cached_semantics_retrieves_without_word_overlap() {
        let snapshot = LessonSnapshot::from_pairs([(
            "project".into(),
            vec![
                ("correction".into(), "Blue backgrounds".into()),
                ("correction".into(), "Reuse verification evidence".into()),
            ],
        )]);
        let cache = LessonRelevance::default();
        {
            let mut state = cache.state.lock().await;
            state
                .queries
                .insert(fingerprint("avoid duplicate checks"), vec![1.0, 0.0]);
            state.documents.insert(2, vec![1.0, 0.0]);
        }
        assert_eq!(
            cache.select(
                &snapshot.for_project("project").iter().collect::<Vec<_>>(),
                "session",
                "avoid duplicate checks",
                1
            )[0]
            .id,
            2
        );
    }

    #[test]
    fn pending_queries_are_bounded_and_repeated_queries_are_coalesced() {
        let mut state = State::default();
        for n in 0..256 {
            queue_query(&mut state, "session", &n.to_string(), "query");
        }
        queue_query(&mut state, "session", "1", "query");
        assert_eq!(state.pending.len(), QUERY_CAPACITY);
    }
    #[test]
    fn context_compaction_rearms_delivery_and_long_queries_still_queue() {
        let snapshot = LessonSnapshot::from_pairs([(
            "project".into(),
            vec![("correction".into(), "Reuse cargo results".into())],
        )]);
        let cache = LessonRelevance::default();
        let candidates = snapshot.for_project("project").iter().collect::<Vec<_>>();
        cache.observe_context("session", 100);
        let selected = cache.select(&candidates, "session", "cargo", 1);
        cache.delivered("session", &selected);
        assert!(cache.select(&candidates, "session", "cargo", 1).is_empty());
        cache.observe_context("session", 2);
        assert_eq!(cache.select(&candidates, "session", "cargo", 1).len(), 1);
        let mut state = State::default();
        queue_query(
            &mut state,
            "session",
            "long",
            &"current context ".repeat(2000),
        );
        assert_eq!(state.pending.len(), 1);
        assert!(state.pending[0].2.len() < 9000);
    }
}
