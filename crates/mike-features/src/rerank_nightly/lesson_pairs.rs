use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::lessons_repo::LessonsPersist as _;
use std::collections::BTreeMap;

use super::grade::rerank_all;
use super::{Idle, Stop, database_failed};
use crate::semantic::KnowledgeState;
use crate::store::persistence::lesson_pair_candidates::NewPairCandidate;
use crate::store::persistence::lesson_relations::CandidateLessonPair;
use crate::store::persistence::{PersistError, PersistHandle};

const THRESHOLD_MARGIN: f64 = 0.10;
const THRESHOLD_CAP: f64 = 0.6;
const PAIRS_PER_PROJECT: usize = 300;

struct Side<'a> {
    query: &'a str,
    documents: Vec<String>,
    targets: Vec<usize>,
}

pub async fn pairs_phase(
    state: &KnowledgeState,
    persist: &PersistHandle,
    idle: &impl Idle,
    found: &mut usize,
) -> Option<Stop> {
    let projects = match persist
        .lessons()
        .list_project_ids_with_embedded_lessons()
        .await
    {
        Ok(projects) => projects,
        Err(error) => {
            return Some(database_failed(
                &error,
                "listing projects with embedded lessons",
            ));
        }
    };
    let threshold = looser_threshold(state.config().lesson_relations.candidate_distance_threshold);
    for project_id in projects {
        match project_pairs(state, persist, idle, project_id, threshold).await {
            Ok(Ok(upserted)) => *found += upserted,
            Ok(Err(stop)) => return Some(stop),
            Err(error) => return Some(database_failed(&error, "lesson pairs")),
        }
    }
    None
}

fn looser_threshold(configured: f64) -> f64 {
    (configured + THRESHOLD_MARGIN).min(THRESHOLD_CAP)
}

async fn project_pairs(
    state: &KnowledgeState,
    persist: &PersistHandle,
    idle: &impl Idle,
    project_id: i64,
    threshold: f64,
) -> Result<Result<usize, Stop>, PersistError> {
    let mut pairs = persist
        .lessons()
        .find_candidate_relation_pairs(project_id, threshold)
        .await?;
    pairs.truncate(PAIRS_PER_PROJECT);
    let min_score = state
        .rerank_service()
        .current_config()
        .nightly
        .lesson_pair_min_score;
    let scores = match score_pairs(state, idle, &pairs).await {
        Ok(scores) => scores,
        Err(stop) => return Ok(Err(stop)),
    };
    let kept: Vec<NewPairCandidate> = pairs
        .iter()
        .zip(scores)
        .filter(|(_, score)| f64::from(*score) >= min_score)
        .map(|(pair, score)| NewPairCandidate {
            lesson_a_id: pair.lesson_a_id,
            lesson_b_id: pair.lesson_b_id,
            score,
            distance: pair.distance,
        })
        .collect();
    Ok(Ok(persist
        .knowledge()
        .upsert_lesson_pair_candidates(&kept)
        .await?))
}

async fn score_pairs(
    state: &KnowledgeState,
    idle: &impl Idle,
    pairs: &[CandidateLessonPair],
) -> Result<Vec<f32>, Stop> {
    let mut best = vec![f32::MIN; pairs.len()];
    for forward in [true, false] {
        for side in sides(pairs, forward) {
            if !idle.is_idle().await {
                return Err(Stop::MachineBusy);
            }
            let Some(scores) = rerank_all(state, side.query, &side.documents).await else {
                return Err(Stop::RerankerFailed);
            };
            for (target, score) in side.targets.iter().zip(scores) {
                let score = score.ok_or(Stop::RerankerFailed)? as f32;
                best[*target] = best[*target].max(score);
            }
        }
    }
    Ok(best)
}

fn sides(pairs: &[CandidateLessonPair], forward: bool) -> Vec<Side<'_>> {
    let mut grouped: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for (index, pair) in pairs.iter().enumerate() {
        let key = if forward {
            pair.lesson_a_id
        } else {
            pair.lesson_b_id
        };
        grouped.entry(key).or_default().push(index);
    }
    grouped
        .into_values()
        .filter_map(|targets| {
            let first = &pairs[*targets.first()?];
            let query = if forward {
                first.lesson_a_text.as_str()
            } else {
                first.lesson_b_text.as_str()
            };
            let documents = targets
                .iter()
                .map(|index| {
                    let pair = &pairs[*index];
                    if forward {
                        pair.lesson_b_text.clone()
                    } else {
                        pair.lesson_a_text.clone()
                    }
                })
                .collect();
            Some(Side {
                query,
                documents,
                targets,
            })
        })
        .collect()
}
