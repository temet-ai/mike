use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use crate::lesson_worker::LessonSnapshot;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeliveredLesson {
    pub id: i64,
    pub kind: String,
    pub text: String,
    pub revision: String,
}

pub struct PromptLessons<'a> {
    pub snapshot: &'a LessonSnapshot,
    pub session_id: &'a str,
    pub project_path: &'a str,
    pub prompt: &'a str,
    pub held: &'a [i64],
    pub held_versions: Option<&'a HashMap<i64, String>>,
    pub limit: usize,
    pub content_only: bool,
}

pub fn select_for_prompt(request: PromptLessons<'_>) -> Vec<DeliveredLesson> {
    let PromptLessons {
        snapshot,
        session_id,
        project_path,
        prompt,
        held,
        held_versions,
        limit,
        content_only,
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
    let chosen = snapshot
        .relevance
        .select(&candidates, session_id, prompt, limit);
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

pub fn catalog_for_prompt(
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
