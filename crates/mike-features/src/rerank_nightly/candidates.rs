use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::cmp::Ordering;
use std::collections::HashSet;

use serde_json::Value;

use super::grade::{embed_query, rerank_all};
use super::{Idle, Stop, database_failed};
use crate::analysis::rerank::MAX_QUERY_CHARS;
use crate::api::project_knowledge::rerank_page::documents_for;
use crate::semantic::KnowledgeState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::project_knowledge::KnowledgeQuery;
use crate::store::persistence::work_threads::{
    CandidateThread, NewThreadCandidate, WorkThreadError,
};

const CANDIDATE_ROWS: i64 = 100;
const LEXICAL_TERMS: usize = 40;
const MIN_TERM_CHARS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Built {
    Written,
    Skipped,
    RerankerFailed,
}

#[derive(Debug, Clone, PartialEq)]
struct Contender {
    reference: String,
    kind: String,
    row: Value,
}

pub async fn candidates_phase(
    state: &KnowledgeState,
    persist: &PersistHandle,
    idle: &impl Idle,
    written: &mut usize,
) -> Option<Stop> {
    let threads = match persist.launch().threads_for_card_candidates().await {
        Ok(threads) => threads,
        Err(error) => {
            return Some(database_failed(
                &error,
                "listing threads for card candidates",
            ));
        }
    };
    for thread in threads {
        if let Some(stop) = visit(state, persist, idle, &thread, written).await {
            return Some(stop);
        }
    }
    None
}

async fn visit(
    state: &KnowledgeState,
    persist: &PersistHandle,
    idle: &impl Idle,
    thread: &CandidateThread,
    written: &mut usize,
) -> Option<Stop> {
    if !idle.is_idle().await {
        return Some(Stop::MachineBusy);
    }
    match build_thread(state, persist, thread).await {
        Ok(Built::Written) => {
            *written += 1;
            None
        }
        Ok(Built::Skipped) => None,
        Ok(Built::RerankerFailed) => Some(Stop::RerankerFailed),
        Err(error) => Some(database_failed(&error, "card candidates")),
    }
}

async fn build_thread(
    state: &KnowledgeState,
    persist: &PersistHandle,
    thread: &CandidateThread,
) -> Result<Built, WorkThreadError> {
    let query = query_text(&thread.query_text);
    if query.is_empty() {
        return Ok(Built::Skipped);
    }
    let vector = embed_query(state, &query).await;
    let input = KnowledgeQuery {
        query: lexical_query(&query),
        semantic_cut: state.rerank_service().ranking().semantic_cut(),
        ..KnowledgeQuery::default()
    };
    let rows = persist
        .knowledge()
        .knowledge_candidate_rows(thread.project_id, &input, CANDIDATE_ROWS, vector.as_ref())
        .await?;
    let contenders = without_own_entries(persist, thread.thread_id, contenders(rows)).await?;
    let rows: Vec<Value> = contenders.iter().map(|c| c.row.clone()).collect();
    let documents = documents_for(persist, &rows, vector.as_ref()).await;
    let Some(scores) = rerank_all(state, &query, &documents).await else {
        return Ok(Built::RerankerFailed);
    };
    if scores.iter().any(Option::is_none) {
        return Ok(Built::RerankerFailed);
    }
    let kept = state
        .rerank_service()
        .current_config()
        .nightly
        .related_per_thread;
    let best = best_of(contenders, &scores, kept);
    persist
        .launch()
        .replace_thread_card_candidates(thread.thread_id, &best)
        .await?;
    Ok(Built::Written)
}

fn query_text(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    joined.chars().take(MAX_QUERY_CHARS).collect()
}

fn lexical_query(text: &str) -> String {
    let mut seen = HashSet::new();
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|term| term.chars().count() >= MIN_TERM_CHARS)
        .map(str::to_lowercase)
        .filter(|term| seen.insert(term.clone()))
        .take(LEXICAL_TERMS)
        .collect::<Vec<_>>()
        .join(" or ")
}

fn contenders(rows: Vec<Value>) -> Vec<Contender> {
    rows.into_iter()
        .filter_map(|row| {
            let source_kind = row["source_kind"].as_str()?;
            let reference = row["reference"].as_str()?;
            let (kind, reference) = match source_kind {
                "lesson" | "note" | "turn" => (source_kind, reference.to_owned()),
                "thread_entry" => ("entry", format!("entry:{}", reference.split_once(':')?.1)),
                _ => return None,
            };
            Some(Contender {
                reference,
                kind: kind.to_owned(),
                row,
            })
        })
        .collect()
}

async fn without_own_entries(
    persist: &PersistHandle,
    thread_id: i64,
    contenders: Vec<Contender>,
) -> Result<Vec<Contender>, WorkThreadError> {
    let entry_ids: Vec<i64> = contenders
        .iter()
        .filter(|c| c.kind == "entry")
        .filter_map(|c| c.reference.strip_prefix("entry:")?.parse().ok())
        .collect();
    if entry_ids.is_empty() {
        return Ok(contenders);
    }
    let own: HashSet<String> = persist
        .launch()
        .thread_entry_ids_of(thread_id, &entry_ids)
        .await?
        .into_iter()
        .map(|id| format!("entry:{id}"))
        .collect();
    Ok(contenders
        .into_iter()
        .filter(|c| !own.contains(&c.reference))
        .collect())
}

fn best_of(
    contenders: Vec<Contender>,
    scores: &[Option<f64>],
    kept: usize,
) -> Vec<NewThreadCandidate> {
    let mut scored: Vec<NewThreadCandidate> = contenders
        .into_iter()
        .zip(scores)
        .filter_map(|(contender, score)| {
            Some(NewThreadCandidate {
                reference: contender.reference,
                kind: contender.kind,
                score: score.map(|s| s as f32)?,
            })
        })
        .collect();
    scored.sort_by(by_score_then_reference);
    scored.truncate(kept);
    scored
}

fn by_score_then_reference(a: &NewThreadCandidate, b: &NewThreadCandidate) -> Ordering {
    b.score
        .total_cmp(&a.score)
        .then_with(|| a.reference.cmp(&b.reference))
}
