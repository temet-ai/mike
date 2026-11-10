use std::collections::HashMap;

use serde_json::{Value, json};

use crate::api::rerank_search::{RerankReport, arrange, candidates_wanted, score};
use crate::config::AppState;
use crate::search_ranking::{Candidate, RankingConfig, order};
use crate::store::persistence::project_knowledge::{KnowledgeQuery, ProjectIdentity, search_page};
use crate::store::persistence::{PersistError, PersistHandle};

const MAX_CANDIDATE_ROWS: i64 = 200;

pub(crate) type QueryVector = (String, String, String);

pub(super) async fn search(
    state: &AppState,
    handle: &PersistHandle,
    project: &ProjectIdentity,
    input: &KnowledgeQuery,
    vector: Option<&QueryVector>,
) -> Result<(Value, RerankReport), PersistError> {
    let ranking = state.rerank_service().ranking();
    let candidates = candidates_wanted(state);
    if candidates.is_none() && ranking.is_default() {
        return plain(handle, input, vector, RerankReport::switched_off()).await;
    }
    if input.query.trim().is_empty() {
        let report = RerankReport::off("there is no query to rerank against");
        return plain(handle, input, vector, report).await;
    }
    let limit = input.limit.unwrap_or(20).clamp(1, 100);
    let offset = input.offset.unwrap_or(0).max(0);
    let window = candidates.unwrap_or_else(|| state.rerank_service().current_config().candidates);
    let wanted = i64::try_from(window)
        .unwrap_or(MAX_CANDIDATE_ROWS)
        .max(offset + limit);
    if wanted > MAX_CANDIDATE_ROWS {
        let report = match candidates {
            Some(_) => RerankReport::off("pages past the first 200 results are not reranked"),
            None => RerankReport::switched_off(),
        };
        return plain(handle, input, vector, report).await;
    }
    let config = state.rerank_service().current_config();
    let rescue_rule = (config.rescue_score, config.rescue_min_kept);
    let rescue = candidates
        .zip(vector)
        .map(|(_, vector)| super::rescue::pool_query(input, &vector.2, rescue_rule));
    let pool = rescue.as_ref().map_or(input, |(pool, _)| pool);
    let pool = super::wide::Pool {
        project,
        input: pool,
        wanted,
        vector,
        reranked: candidates.is_some(),
    };
    let rows = super::wide::candidate_rows(state, handle, pool).await?;
    let rows = if ranking.is_default() {
        rows
    } else {
        reorder_rows(rows, &ranking)
    };
    let (arranged, report) = match candidates {
        Some(candidates) => {
            match rerank_rows(state, handle, input, vector, candidates, rows).await {
                Ok((arranged, report)) => match &rescue {
                    Some((_, cut)) => (super::rescue::keep(arranged, *cut), report),
                    None => (arranged, report),
                },
                Err(report) => return plain(handle, input, vector, report).await,
            }
        }
        None => (arrange(rows, &[]), RerankReport::switched_off()),
    };
    let page: Vec<Value> = arranged
        .into_iter()
        .skip(usize::try_from(offset).unwrap_or(0))
        .take(usize::try_from(limit + 1).unwrap_or(1))
        .map(|(item, score)| with_rerank_score(item, score))
        .collect();
    Ok((
        search_page(project.project_uuid, vector.is_some(), page, limit, offset),
        report,
    ))
}

fn reorder_rows(rows: Vec<Value>, ranking: &RankingConfig) -> Vec<Value> {
    let candidates: Vec<Candidate> = rows.iter().map(Candidate::from_row).collect();
    let mut slots: Vec<Option<Value>> = rows.into_iter().map(Some).collect();
    order(&candidates, ranking)
        .into_iter()
        .filter_map(|index| slots.get_mut(index).and_then(Option::take))
        .collect()
}

async fn plain(
    handle: &PersistHandle,
    input: &KnowledgeQuery,
    vector: Option<&QueryVector>,
    report: RerankReport,
) -> Result<(Value, RerankReport), PersistError> {
    let page = handle
        .search_project_knowledge_hybrid(input, vector)
        .await?;
    Ok((page, report))
}

async fn rerank_rows(
    state: &AppState,
    handle: &PersistHandle,
    input: &KnowledgeQuery,
    vector: Option<&QueryVector>,
    candidates: usize,
    rows: Vec<Value>,
) -> Result<(Vec<(Value, Option<f32>)>, RerankReport), RerankReport> {
    let window = rows.len().min(candidates);
    if window == 0 {
        let report = RerankReport::off("there were no results to rerank");
        return Ok((arrange(rows, &[]), report));
    }
    let documents = documents_for(handle, &rows[..window], vector).await;
    let scores = score(state, &input.query, &documents).await?;
    Ok((arrange(rows, &scores), RerankReport::reranked()))
}

pub(crate) async fn documents_for(
    handle: &PersistHandle,
    rows: &[Value],
    vector: Option<&QueryVector>,
) -> Vec<String> {
    let references: Vec<String> = rows
        .iter()
        .filter_map(|row| row["reference"].as_str())
        .map(str::to_owned)
        .collect();
    let passages = match handle.knowledge_rerank_passages(&references, vector).await {
        Ok(passages) => passages,
        Err(error) => {
            tracing::warn!(error = %error, "knowledge rerank: passage lookup failed; ranking on titles");
            HashMap::new()
        }
    };
    rows.iter().map(|row| document(row, &passages)).collect()
}

fn document(row: &Value, passages: &HashMap<String, String>) -> String {
    let title = row["title"].as_str().unwrap_or_default();
    match row["reference"]
        .as_str()
        .and_then(|reference| passages.get(reference))
    {
        Some(passage) => format!("{title}\n\n{passage}"),
        None => title.to_owned(),
    }
}

fn with_rerank_score(mut item: Value, score: Option<f32>) -> Value {
    if let Some(score) = score {
        item["rerank_score"] = json!((f64::from(score) * 10_000.0).round() / 10_000.0);
    }
    item
}
