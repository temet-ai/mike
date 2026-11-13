use crate::store::persistence::lessons_repo::LessonsPersist as _;
use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::ProposerDeps;
use crate::store::persistence::PersistError;
use crate::store::persistence::lessons::placement::{SkillPurpose, TopicMove};

const FILLER_WORDS: [&str; 10] = [
    "the", "a", "an", "of", "and", "for", "to", "in", "on", "with",
];
const MIN_SHARED_WORDS: usize = 2;

#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct PurposeReport {
    pub merged_topics: u64,
    pub moved_lessons: u64,
}

fn stem(word: &str) -> String {
    let chars = word.chars().count();
    if chars > 5 && word.ends_with("ing") {
        return word[..word.len() - 3].to_owned();
    }
    if chars > 4 && word.ends_with("ed") {
        return word[..word.len() - 2].to_owned();
    }
    if chars > 3 && word.ends_with('s') && !word.ends_with("ss") {
        return word[..word.len() - 1].to_owned();
    }
    word.to_owned()
}

fn words(topic: &str) -> BTreeSet<String> {
    topic
        .split(|c: char| !c.is_ascii_alphanumeric())
        .map(str::to_lowercase)
        .filter(|word| !word.is_empty() && !FILLER_WORDS.contains(&word.as_str()))
        .map(|word| stem(&word))
        .collect()
}

pub fn same_purpose(first: &str, second: &str) -> bool {
    let (a, b) = (words(first), words(second));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let shared = a.intersection(&b).count();
    shared >= MIN_SHARED_WORDS && shared * 4 >= a.len().min(b.len()) * 3
}

pub fn plan_moves(purposes: &[SkillPurpose]) -> BTreeMap<i64, Vec<TopicMove>> {
    let mut by_project: BTreeMap<i64, Vec<&SkillPurpose>> = BTreeMap::new();
    for purpose in purposes {
        by_project
            .entry(purpose.project_id)
            .or_default()
            .push(purpose);
    }
    let mut plan = BTreeMap::new();
    for (project_id, mut topics) in by_project {
        topics.sort_by(|a, b| {
            b.shipped
                .cmp(&a.shipped)
                .then(b.lessons.cmp(&a.lessons))
                .then(a.topic.cmp(&b.topic))
        });
        let mut survivors: Vec<&str> = Vec::new();
        let mut moves = Vec::new();
        for topic in topics {
            match survivors
                .iter()
                .find(|survivor| same_purpose(survivor, &topic.topic))
            {
                Some(survivor) => moves.push(TopicMove {
                    from: topic.topic.clone(),
                    to: (*survivor).to_owned(),
                }),
                None => survivors.push(&topic.topic),
            }
        }
        if !moves.is_empty() {
            plan.insert(project_id, moves);
        }
    }
    plan
}

fn log_read_failed(error: &PersistError) {
    tracing::warn!(%error, "lesson_proposer: purpose pass could not read skill topics");
}

fn log_merged(project_id: i64, merged: usize, moved: u64) {
    tracing::info!(
        project_id,
        merged,
        moved,
        "lesson_proposer: skill topics with the same purpose merged"
    );
}

fn log_merge_failed(project_id: i64, error: &PersistError) {
    tracing::warn!(%error, project_id, "lesson_proposer: purpose merge failed");
}

async fn merge_project(
    deps: &ProposerDeps<'_>,
    project_id: i64,
    moves: &[TopicMove],
    report: &mut PurposeReport,
) {
    match deps
        .persist
        .lessons()
        .retopic_skill_lessons(project_id, moves)
        .await
    {
        Ok(moved) => {
            report.merged_topics += moves.len() as u64;
            report.moved_lessons += moved;
            log_merged(project_id, moves.len(), moved);
        }
        Err(error) => log_merge_failed(project_id, &error),
    }
}

pub async fn run_pass(deps: &ProposerDeps<'_>) -> PurposeReport {
    let mut report = PurposeReport::default();
    let purposes = match deps.persist.lessons().skill_purposes().await {
        Ok(purposes) => purposes,
        Err(error) => {
            log_read_failed(&error);
            return report;
        }
    };
    for (project_id, moves) in plan_moves(&purposes) {
        merge_project(deps, project_id, &moves, &mut report).await;
    }
    report
}

#[cfg(test)]
#[path = "purpose_tests.rs"]
mod tests;
