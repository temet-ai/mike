use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use serde_json::Value;

use super::first_ten::{Scored, first_ten_references, grade_first_ten, turn_query};
use super::search_decide::SearchJev;
use crate::api::project_knowledge::rerank_page::documents_for;
use crate::api::project_knowledge::retrieval::query_vector;
use crate::api::rerank_search::score;
use crate::embed_worker::knowledge::model_key;
use crate::search_ranking::Candidate;
use crate::search_ranking::tuning::{GradeCandidate, reranked_order};
use crate::semantic::KnowledgeState;
use crate::store::persistence::project_knowledge::KnowledgeQuery;
use crate::store::persistence::search_grades::{NewGrade, UngradedEvent};
use crate::store::persistence::{PersistError, PersistHandle};

const CANDIDATE_ROWS: i64 = 200;
const CHUNK_DOCUMENTS: usize = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Graded {
    Recorded,
    RerankerFailed,
}

pub async fn grade_event(
    state: &KnowledgeState,
    persist: &PersistHandle,
    event: &UngradedEvent,
    jev: Option<&SearchJev>,
) -> Result<Graded, PersistError> {
    let vector = embed_query(state, &event.query).await;
    let input = KnowledgeQuery {
        project: event.project_uuid.to_string(),
        query: event.query.clone(),
        kind: event.kind_filter.clone(),
        semantic_cut: state.rerank_service().ranking().semantic_cut(),
        ..KnowledgeQuery::default()
    };
    let mut rows = persist
        .knowledge()
        .knowledge_candidate_rows(event.project_id, &input, CANDIDATE_ROWS, vector.as_ref())
        .await?;
    rows.truncate(usize::try_from(CANDIDATE_ROWS).unwrap_or(usize::MAX));
    let documents = documents_for(persist, &rows, vector.as_ref()).await;
    let Some(scores) = rerank_all(state, &event.query, &documents).await else {
        return Ok(Graded::RerankerFailed);
    };
    let candidates = grade_candidates(&rows, &scores);
    let window = state.rerank_service().current_config().candidates;
    let stored = serde_json::to_value(&candidates).unwrap_or(Value::Null);
    let plain = plain_references(&candidates);
    let reranked = reranked_references(&candidates, window);
    let turn = turn_query(&event.query, event.turn_prompt.as_deref());
    let first = grade_first_ten(
        state,
        &first_ten_references(&event.returned),
        turn.as_deref(),
        &Scored {
            candidates: &candidates,
            documents: &documents,
            jev,
            query: &event.query,
            turn_prompt: event.turn_prompt.as_deref(),
            used: &event.used_references,
        },
    )
    .await;
    let first_ten = serde_json::to_value(&first.results).unwrap_or(Value::Array(Vec::new()));
    persist
        .knowledge()
        .insert_search_grade(&NewGrade {
            event_id: event.event_id,
            candidates: &stored,
            used_references: &event.used_references,
            plain_positions: &positions(&event.used_references, &plain),
            reranked_positions: &positions(&event.used_references, &reranked),
            first_ten: &first_ten,
            turn_prompt: event.turn_prompt.as_deref(),
            relevant_references: &first.relevant,
        })
        .await?;
    Ok(Graded::Recorded)
}

pub async fn embed_query(
    state: &KnowledgeState,
    query: &str,
) -> Option<crate::api::project_knowledge::rerank_page::QueryVector> {
    let service = state.embedding_service();
    if query.trim().is_empty() || !service.is_available() {
        return None;
    }
    query_vector(&service, &model_key(&service), query).await
}

pub async fn rerank_all(
    state: &KnowledgeState,
    query: &str,
    documents: &[String],
) -> Option<Vec<Option<f64>>> {
    let mut scores: Vec<Option<f64>> = vec![None; documents.len()];
    let mut answered = documents.is_empty();
    for (chunk_index, chunk) in documents.chunks(CHUNK_DOCUMENTS).enumerate() {
        let base = chunk_index * CHUNK_DOCUMENTS;
        match score(state, query, chunk).await {
            Ok(chunk_scores) => {
                answered = true;
                for chunk_score in chunk_scores.scores {
                    if let Some(slot) = scores.get_mut(base + chunk_score.index) {
                        *slot = Some(f64::from(chunk_score.score));
                    }
                }
            }
            Err(report) => {
                tracing::debug!(detail = ?report.detail, "rerank_nightly: a chunk was not scored");
            }
        }
    }
    answered.then_some(scores)
}

fn grade_candidates(rows: &[Value], scores: &[Option<f64>]) -> Vec<GradeCandidate> {
    rows.iter()
        .zip(scores)
        .map(|(row, rerank_score)| {
            let candidate = Candidate::from_row(row);
            GradeCandidate {
                reference: candidate.reference,
                kind: candidate.kind,
                lexical: candidate.lexical,
                semantic: candidate.semantic,
                rerank_score: *rerank_score,
            }
        })
        .collect()
}

fn plain_references(candidates: &[GradeCandidate]) -> Vec<String> {
    candidates.iter().map(|c| c.reference.clone()).collect()
}

fn reranked_references(candidates: &[GradeCandidate], window: usize) -> Vec<String> {
    let scores: Vec<Option<f64>> = candidates.iter().map(|c| c.rerank_score).collect();
    let identity: Vec<usize> = (0..candidates.len()).collect();
    reranked_order(identity, &scores, window)
        .into_iter()
        .map(|index| candidates[index].reference.clone())
        .collect()
}

pub fn positions(used: &[String], ordered: &[String]) -> Vec<i32> {
    used.iter()
        .map(|reference| {
            ordered
                .iter()
                .position(|candidate| candidate == reference)
                .and_then(|index| i32::try_from(index + 1).ok())
                .unwrap_or(0)
        })
        .collect()
}
