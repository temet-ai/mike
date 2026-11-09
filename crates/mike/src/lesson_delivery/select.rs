use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use crate::lesson_worker::LessonSnapshot;
use crate::lesson_worker::relevance::DEFAULT_LESSON_BUDGET;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct DeliveredLesson {
    pub(crate) id: i64,
    pub(crate) kind: String,
    pub(crate) text: String,
    pub(crate) revision: String,
}

pub(crate) struct PromptLessons<'a> {
    pub(crate) snapshot: &'a LessonSnapshot,
    pub(crate) session_id: &'a str,
    pub(crate) project_path: &'a str,
    pub(crate) prompt: &'a str,
    pub(crate) held: &'a [i64],
    pub(crate) held_versions: Option<&'a HashMap<i64, String>>,
    pub(crate) limit: usize,
    pub(crate) content_only: bool,
    pub(crate) budget: Option<usize>,
}

pub(crate) fn select_for_prompt(request: PromptLessons<'_>) -> Vec<DeliveredLesson> {
    let PromptLessons {
        snapshot,
        session_id,
        project_path,
        prompt,
        held,
        held_versions,
        limit,
        content_only,
        budget,
    } = request;
    let mut seen = std::collections::HashSet::new();
    let candidates: Vec<_> = snapshot
        .shared
        .iter()
        .chain(
            snapshot
                .for_project(snapshot.project_for_session(session_id, project_path))
                .iter(),
        )
        .filter(|lesson| {
            held_versions.map_or_else(
                || !held.contains(&lesson.id),
                |versions| versions.get(&lesson.id) != Some(&revision(lesson)),
            )
        })
        .filter(|lesson| !content_only || !snapshot.withholds_content(lesson))
        .filter(|lesson| seen.insert(lesson.id))
        .collect();
    let chosen = snapshot.relevance.select_within(
        &candidates,
        session_id,
        prompt,
        limit,
        budget.unwrap_or(DEFAULT_LESSON_BUDGET),
    );
    snapshot.relevance.delivered(session_id, &chosen);
    chosen
        .into_iter()
        .map(|lesson| DeliveredLesson {
            id: lesson.id,
            kind: lesson.kind.clone(),
            text: lesson.text.trim().to_owned(),
            revision: revision(lesson),
        })
        .collect()
}

fn revision(lesson: &crate::lesson_worker::InjectableLesson) -> String {
    hex::encode(Sha256::digest(
        format!("{}:{}:{:?}", lesson.kind, lesson.text, lesson.topic).as_bytes(),
    ))
}

pub(crate) fn catalog_for_prompt(
    snapshot: &LessonSnapshot,
    session: &str,
    project: &str,
    prompt: &str,
) -> Vec<String> {
    let candidates: Vec<_> = snapshot
        .shared
        .iter()
        .chain(snapshot.for_project(snapshot.project_for_session(session, project)))
        .filter(|lesson| snapshot.withholds_content(lesson))
        .collect();
    let selected = snapshot.relevance.select(&candidates, session, prompt, 3);
    let mut topics: Vec<_> = selected
        .into_iter()
        .filter_map(|lesson| lesson.topic.clone())
        .collect();
    topics.sort();
    topics.dedup();
    topics
}
