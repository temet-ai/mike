use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use super::held::{HeldTag, held_lesson_tags, is_held};
use crate::conversation::EarlierTurns;
use crate::lesson_worker::{InjectableLesson, LessonSnapshot};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeliveredLesson {
    pub id: i64,
    pub kind: String,
    pub text: String,
    pub revision: String,
    pub rev: i32,
}

pub struct PromptLessons<'a> {
    pub snapshot: &'a LessonSnapshot,
    pub session_id: &'a str,
    pub project_path: &'a str,
    pub prompt: &'a str,
    pub held: &'a [i64],
    pub held_versions: Option<&'a HashMap<i64, String>>,
    pub held_tags: &'a [HeldTag],
    pub limit: usize,
    pub content_only: bool,
}

fn project_pool<'a>(
    snapshot: &'a LessonSnapshot,
    session_id: &str,
    project_path: &str,
) -> impl Iterator<Item = &'a InjectableLesson> + use<'a> {
    let project = snapshot
        .project_for_session(session_id, project_path)
        .to_owned();
    snapshot
        .shared
        .iter()
        .chain(snapshot.for_project(&project).iter())
}

pub fn already_present_ids(
    snapshot: &LessonSnapshot,
    session_id: &str,
    project_path: &str,
    body: &[u8],
    earlier: &EarlierTurns,
) -> Vec<i64> {
    let tags = held_lesson_tags(body, earlier);
    let mut ids = Vec::new();
    for lesson in project_pool(snapshot, session_id, project_path) {
        let present = !lesson.route.is_injection() || is_held(&tags, lesson);
        if present && !ids.contains(&lesson.id) {
            ids.push(lesson.id);
        }
    }
    ids
}

pub fn select_for_prompt(request: PromptLessons<'_>) -> Vec<DeliveredLesson> {
    let PromptLessons {
        snapshot,
        session_id,
        project_path,
        prompt,
        held,
        held_versions,
        held_tags,
        limit,
        content_only,
    } = request;
    let mut seen = std::collections::HashSet::new();
    let candidates: Vec<_> = project_pool(snapshot, session_id, project_path)
        .filter(|lesson| lesson.route.is_injection() && !is_held(held_tags, lesson))
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
            rev: lesson.revision,
        })
        .collect()
}

fn revision(lesson: &InjectableLesson) -> String {
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
