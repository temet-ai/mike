use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use serde_json::{Value, json};

use super::item_header;
use super::passage_excerpt::{Passages, show_reranker_passages};
use crate::api::rerank_search::{
    RerankReport, arrange, candidates_wanted, score, short_query_report,
};
use crate::search_ranking::{
    Candidate, RankingConfig, lift_identifier_rows, order, order_by_position, weigh_outside_project,
};
use crate::state::AppState;
use crate::store::persistence::project_knowledge::{KnowledgeQuery, ProjectIdentity, search_page};
use crate::store::persistence::{PersistError, PersistHandle};

pub(super) const MAX_CANDIDATE_ROWS: i64 = 200;

pub(crate) type QueryVector = crate::store::persistence::project_knowledge::QueryEmbedding;

pub(super) type Searched = (Value, RerankReport, Passages);

pub(super) async fn search(
    state: &AppState,
    handle: &PersistHandle,
    project: &ProjectIdentity,
    input: &KnowledgeQuery,
    vector: Option<&QueryVector>,
) -> Result<Searched, PersistError> {
    if input.recent {
        return super::recent::search(state, handle, project, input, vector).await;
    }
    let ranking = state.rerank_service().ranking();
    let candidates = candidates_wanted(state);
    if candidates.is_none() && ranking.is_default() {
        return plain(handle, project, input, vector, RerankReport::switched_off()).await;
    }
    if input.query.trim().is_empty() {
        let report = RerankReport::off("there is no query to rerank against");
        return plain(handle, project, input, vector, report).await;
    }
    if let Some(report) = candidates.and_then(|_| short_query_report(state, &input.query)) {
        return plain(handle, project, input, vector, report).await;
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
        return plain(handle, project, input, vector, report).await;
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
    let stage = Stage {
        candidates,
        rescue: rescue.as_ref().map(|(_, cut)| *cut),
    };
    let (arranged, report, passages) =
        match arrange_stage(state, handle, (input, vector), stage, rows).await {
            Ok(staged) => staged,
            Err(report) => return plain(handle, project, input, vector, report).await,
        };
    let arranged = if candidates.is_some() {
        weigh_by_writer(arranged, &ranking)
    } else {
        arranged
    };
    let mut page: Vec<Value> = lift_identifier_rows(arranged)
        .into_iter()
        .skip(usize::try_from(offset).unwrap_or(0))
        .take(usize::try_from(limit + 1).unwrap_or(1))
        .map(|(item, score)| with_rerank_score(item, score))
        .collect();
    show_reranker_passages(&mut page, &passages);
    Ok((
        search_page(project.project_uuid, vector.is_some(), page, limit, offset),
        report,
        passages,
    ))
}

struct Stage {
    candidates: Option<usize>,
    rescue: Option<super::rescue::Rescue>,
}

type Staged = (Vec<(Value, Option<f32>)>, RerankReport, Passages);

async fn arrange_stage(
    state: &AppState,
    handle: &PersistHandle,
    (input, vector): (&KnowledgeQuery, Option<&QueryVector>),
    stage: Stage,
    rows: Vec<Value>,
) -> Result<Staged, RerankReport> {
    let Some(candidates) = stage.candidates else {
        let report = RerankReport::switched_off();
        return Ok((arrange(rows, &[]), report, Passages::new()));
    };
    let (arranged, report, passages) =
        rerank_rows(state, handle, input, vector, candidates, rows).await?;
    Ok(match stage.rescue {
        Some(cut) => {
            let (kept, report) = super::rescue::keep_reporting(arranged, cut, report);
            (kept, report, passages)
        }
        None => (arranged, report, passages),
    })
}

pub(super) fn weigh_by_writer(
    arranged: Vec<(Value, Option<f32>)>,
    ranking: &RankingConfig,
) -> Vec<(Value, Option<f32>)> {
    let arranged = weigh_outside_project(arranged);
    let candidates: Vec<Candidate> = arranged
        .iter()
        .map(|(row, _)| Candidate::from_row(row))
        .collect();
    let mut slots: Vec<Option<(Value, Option<f32>)>> = arranged.into_iter().map(Some).collect();
    order_by_position(&candidates, ranking)
        .into_iter()
        .filter_map(|index| slots.get_mut(index).and_then(Option::take))
        .collect()
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
    project: &ProjectIdentity,
    input: &KnowledgeQuery,
    vector: Option<&QueryVector>,
    report: RerankReport,
) -> Result<Searched, PersistError> {
    let page = handle
        .knowledge()
        .search_resolved_knowledge(project, input, vector)
        .await?;
    Ok((page, report, Passages::new()))
}

async fn rerank_rows(
    state: &AppState,
    handle: &PersistHandle,
    input: &KnowledgeQuery,
    vector: Option<&QueryVector>,
    candidates: usize,
    rows: Vec<Value>,
) -> Result<Staged, RerankReport> {
    let window = rows.len().min(candidates);
    if window == 0 {
        let report = RerankReport::off("there were no results to rerank");
        return Ok((arrange(rows, &[]), report, Passages::new()));
    }
    let passages = passages_for(handle, &rows[..window], vector).await;
    let documents = documents_from(&rows[..window], &passages);
    let reranked = score(state, &input.query, &documents).await?;
    let report = RerankReport::reranked().with_cached(reranked.cached);
    Ok((arrange(rows, &reranked.scores), report, passages))
}

pub(crate) async fn documents_for(
    handle: &PersistHandle,
    rows: &[Value],
    vector: Option<&QueryVector>,
) -> Vec<String> {
    let passages = passages_for(handle, rows, vector).await;
    documents_from(rows, &passages)
}

fn documents_from(rows: &[Value], passages: &Passages) -> Vec<String> {
    rows.iter().map(|row| document(row, passages)).collect()
}

async fn passages_for(
    handle: &PersistHandle,
    rows: &[Value],
    vector: Option<&QueryVector>,
) -> Passages {
    let references: Vec<String> = rows
        .iter()
        .filter_map(|row| row["reference"].as_str())
        .map(str::to_owned)
        .collect();
    match handle
        .knowledge()
        .knowledge_rerank_passages(&references, vector)
        .await
    {
        Ok(passages) => passages,
        Err(error) => {
            tracing::warn!(error = %error, "knowledge rerank: passage lookup failed; ranking on titles");
            Passages::new()
        }
    }
}

fn document(row: &Value, passages: &Passages) -> String {
    let title = row["title"].as_str().unwrap_or_default();
    let body = match row["reference"]
        .as_str()
        .and_then(|reference| passages.get(reference))
    {
        Some(passage) => format!("{title}\n\n{passage}"),
        None => title.to_owned(),
    };
    item_header::with_header(&item_header::header(row), &body)
}

fn with_rerank_score(mut item: Value, score: Option<f32>) -> Value {
    if let Some(score) = score {
        item["rerank_score"] = json!((f64::from(score) * 10_000.0).round() / 10_000.0);
    }
    item
}
