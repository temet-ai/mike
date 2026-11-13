use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::LessonCache;
use super::snapshot::{self, InjectableLesson, LessonSet, LessonSnapshot};
use crate::store::persistence::lessons::ChangeMark;
use crate::store::persistence::{PersistError, PersistHandle};

struct SessionProjects {
    fingerprint: ChangeMark,
    paths: HashMap<String, String>,
}

struct Refreshed {
    lessons: Option<LessonSet>,
    sessions: Option<SessionProjects>,
    persona_alias_map: HashMap<String, i64>,
    topic_withheld: Arc<HashSet<i64>>,
}

impl Refreshed {
    fn changes(&self, previous: &LessonSnapshot) -> bool {
        self.lessons.is_some()
            || self.sessions.is_some()
            || self.persona_alias_map != previous.persona_alias_map
            || *self.topic_withheld != *previous.topic_withheld
    }
}

pub async fn refresh_lesson_cache_inner(persist: &PersistHandle, cache: &LessonCache) {
    let previous = cache.load_full();
    let Ok(lessons) = snapshot::load_changed_lessons(persist, previous.lessons_fingerprint)
        .await
        .inspect_err(log_refresh_snapshot_failed)
    else {
        return;
    };
    let persona_alias_map = match snapshot::load_persona_alias_map(persist).await {
        Ok(m) => m,
        Err(e) => {
            log_refresh_alias_map_failed(&e);
            previous.persona_alias_map.clone()
        }
    };
    let topic_withheld = match snapshot::load_topic_withheld(persist).await {
        Ok(ids) => Arc::new(ids),
        Err(e) => {
            log_refresh_topic_withheld_failed(&e);
            previous.topic_withheld.clone()
        }
    };
    let Ok(sessions) = load_changed_sessions(persist, previous.sessions_fingerprint)
        .await
        .inspect_err(log_refresh_sessions_failed)
    else {
        return;
    };
    let refreshed = Refreshed {
        lessons,
        sessions,
        persona_alias_map,
        topic_withheld,
    };
    if !refreshed.changes(&previous) {
        return;
    }
    let next = rebuild(&previous, refreshed);
    log_snapshot_refreshed(next.project_count());
    cache.store(Arc::new(next));
}

async fn load_changed_sessions(
    persist: &PersistHandle,
    previous: Option<ChangeMark>,
) -> Result<Option<SessionProjects>, PersistError> {
    let fingerprint = persist
        .knowledge()
        .lesson_session_projects_fingerprint()
        .await?;
    if previous == Some(fingerprint) {
        return Ok(None);
    }
    let paths = persist.knowledge().lesson_session_projects().await?;
    Ok(Some(SessionProjects { fingerprint, paths }))
}

fn rebuild(previous: &LessonSnapshot, refreshed: Refreshed) -> LessonSnapshot {
    let (by_project, by_agent, shared, lessons_fingerprint) = match refreshed.lessons {
        Some(set) => {
            let shared = shared_lessons(&set.by_project);
            (set.by_project, set.by_agent, shared, Some(set.fingerprint))
        }
        None => (
            previous.by_project.clone(),
            previous.by_agent.clone(),
            previous.shared.clone(),
            previous.lessons_fingerprint,
        ),
    };
    let (session_project_paths, sessions_fingerprint) = match refreshed.sessions {
        Some(sessions) => (sessions.paths, Some(sessions.fingerprint)),
        None => (
            previous.session_project_paths.clone(),
            previous.sessions_fingerprint,
        ),
    };
    LessonSnapshot {
        shared,
        relevance: previous.relevance.clone(),
        session_project_paths,
        by_project,
        by_agent,
        persona_alias_map: refreshed.persona_alias_map,
        topic_withheld: refreshed.topic_withheld,
        agent_lesson_injected: previous.agent_lesson_injected.clone(),
        lessons_fingerprint,
        sessions_fingerprint,
    }
}

fn shared_lessons(by_project: &HashMap<String, Vec<InjectableLesson>>) -> Vec<InjectableLesson> {
    by_project
        .values()
        .flatten()
        .filter(|lesson| {
            lesson
                .topic
                .as_deref()
                .is_some_and(|topic| topic.starts_with("shared:"))
        })
        .map(|lesson| {
            let mut shared = lesson.clone();
            shared.topic = None;
            shared
        })
        .collect()
}

fn log_refresh_sessions_failed(error: &PersistError) {
    tracing::warn!(%error, "Unable to refresh session project identities");
}

fn log_refresh_topic_withheld_failed(e: &PersistError) {
    tracing::warn!(
        error = %e,
        "lesson_worker: topic-withheld lesson ids failed during snapshot rebuild — keeping previous set"
    );
}

fn log_refresh_snapshot_failed(e: &PersistError) {
    tracing::warn!(
        error = %e,
        "lesson_worker: failed to refresh injection snapshot — keeping previous snapshot"
    );
}

fn log_refresh_alias_map_failed(e: &PersistError) {
    tracing::warn!(
        error = %e,
        "lesson_worker: fetch_alias_map failed during snapshot rebuild — keeping previous alias map"
    );
}

fn log_snapshot_refreshed(project_count: usize) {
    tracing::debug!(project_count, "lesson_worker: injection snapshot refreshed");
}
