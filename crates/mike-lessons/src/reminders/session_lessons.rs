use crate::extract::request::TurnOrigin;
use crate::lesson_worker::{InjectableLesson, LessonSnapshot};
use crate::proxy::remote_control::inject::PushDeps;
use crate::store::models::{ReminderSource, RequestId};

use super::{RemindersConfig, RemindersOutcome, formatter, push_or_append_reminder};

pub struct SessionLessonsRequest<'a> {
    pub body_bytes: &'a [u8],
    pub session_id: &'a str,
    pub project_path: &'a str,
    pub is_tool_loop: bool,
    pub turn_origin: &'a TurnOrigin,
    pub snapshot: &'a LessonSnapshot,
    pub cfg: &'a RemindersConfig,
    pub recall_injector_enabled: bool,
    pub request_id: RequestId,
    pub push_deps: &'a PushDeps,
}

pub async fn apply_session_lessons(req: SessionLessonsRequest<'_>) -> RemindersOutcome {
    apply_session_lessons_with_floor(req, None).await
}

pub async fn apply_session_lessons_with_floor(
    req: SessionLessonsRequest<'_>,
    floor: Option<&crate::llm::decisions::point::Decider>,
) -> RemindersOutcome {
    let SessionLessonsRequest {
        body_bytes,
        session_id,
        project_path,
        is_tool_loop,
        turn_origin,
        snapshot,
        cfg,
        recall_injector_enabled,
        request_id,
        push_deps,
    } = req;

    if !cfg.inject_lessons {
        return RemindersOutcome::Unchanged;
    }

    if project_path.is_empty() {
        return RemindersOutcome::Unchanged;
    }

    let (query, message_count) = super::session_lessons_query::conversation_query(body_bytes);
    if message_count > 0 {
        snapshot
            .relevance
            .observe_context(session_id, message_count);
    }
    if !matches!(turn_origin, TurnOrigin::Primary | TurnOrigin::PrimaryFirst)
        || (is_tool_loop && !snapshot.relevance.query_ready(session_id, &query))
    {
        return RemindersOutcome::Unchanged;
    }
    let mut seen = std::collections::HashSet::new();
    let candidates: Vec<_> = snapshot
        .shared
        .iter()
        .chain(
            snapshot
                .for_project(snapshot.project_for_session(session_id, project_path))
                .iter(),
        )
        .filter(|lesson| seen.insert(lesson.id))
        .filter(|lesson| floor.is_none() || !super::supply_floor::is_held(session_id, lesson.id))
        .collect();
    let selected =
        snapshot
            .relevance
            .select(&candidates, session_id, &query, cfg.max_lessons_to_inject);
    let all_lessons = match floor {
        Some(decider) => {
            let context = super::supply_floor::Context {
                session: session_id,
                request_id: i64::try_from(request_id.as_u64()).unwrap_or(0),
                text: &query,
            };
            super::supply_floor::keep_above_floor(decider, &context, selected).await
        }
        None => selected,
    };

    if all_lessons.is_empty() {
        return RemindersOutcome::Unchanged;
    }

    let injected_lesson_ids =
        content_injected_lesson_ids(&all_lessons, snapshot, recall_injector_enabled);

    let Some(reminder_text) = build_session_lesson_reminder_text(
        session_id,
        &all_lessons,
        snapshot,
        recall_injector_enabled,
    ) else {
        return RemindersOutcome::Unchanged;
    };

    let outcome = push_or_append_reminder(super::PushOrAppendRequest {
        push_deps,
        session_id,
        body_bytes,
        source: ReminderSource::Lesson,
        reminder_text,
        source_finding_ids: injected_lesson_ids,
        source_request_id: 0,
        request_id,
    })
    .await;
    if matches!(
        outcome,
        RemindersOutcome::Appended { .. } | RemindersOutcome::DeliveredOutOfBand { .. }
    ) {
        snapshot.relevance.delivered(session_id, &all_lessons);
    }
    outcome
}

fn catalog_topics(all_lessons: &[&InjectableLesson], snapshot: &LessonSnapshot) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    all_lessons
        .iter()
        .filter(|lesson| snapshot.withholds_content(lesson))
        .filter_map(|lesson| lesson.topic.clone())
        .filter(|topic| seen.insert(topic.clone()))
        .collect()
}

pub fn build_session_lesson_reminder_text(
    session_id: &str,
    all_lessons: &[&InjectableLesson],
    snapshot: &LessonSnapshot,
    recall_injector_enabled: bool,
) -> Option<String> {
    if !recall_injector_enabled {
        let injectable: Vec<(String, String)> = all_lessons
            .iter()
            .map(|l| (l.kind.clone(), l.text.clone()))
            .collect();
        return formatter::build_lessons_reminder_text(&injectable);
    }

    let unique_topics = catalog_topics(all_lessons, snapshot);
    let content_lessons: Vec<(String, String)> = all_lessons
        .iter()
        .filter(|l| !snapshot.withholds_content(l))
        .map(|l| (l.kind.clone(), l.text.clone()))
        .collect();

    match (unique_topics.is_empty(), content_lessons.is_empty()) {
        (false, true) => {
            crate::recall_injector::formatter::memory_note_for(session_id, &unique_topics)
        }
        (true, false) => formatter::build_lessons_reminder_text(&content_lessons),
        (false, false) => {
            let mut parts = Vec::new();
            if let Some(content_block) = formatter::build_lessons_reminder_text(&content_lessons) {
                parts.push(content_block);
            }
            if let Some(catalog) =
                crate::recall_injector::formatter::memory_note_for(session_id, &unique_topics)
            {
                parts.push(catalog);
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        (true, true) => None,
    }
}

pub fn content_injected_lesson_ids(
    all_lessons: &[&InjectableLesson],
    snapshot: &LessonSnapshot,
    recall_injector_enabled: bool,
) -> Vec<i64> {
    all_lessons
        .iter()
        .filter(|l| !recall_injector_enabled || !snapshot.withholds_content(l))
        .map(|l| l.id)
        .collect()
}
