use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use std::collections::HashSet;

use serde_json::Value;

use crate::search_ranking::{identifier_hits, mark_outside_project};
use crate::semantic::KnowledgeState;
use crate::store::persistence::project_knowledge::{
    KnowledgeQuery, ProjectIdentity, QueryEmbedding,
};
use crate::store::persistence::{PersistError, PersistHandle};

pub struct Pool<'a> {
    pub project: &'a ProjectIdentity,
    pub input: &'a KnowledgeQuery,
    pub wanted: i64,
    pub vector: Option<&'a QueryEmbedding>,
    pub reranked: bool,
}

pub async fn candidate_rows(
    state: &KnowledgeState,
    handle: &PersistHandle,
    pool: Pool<'_>,
) -> Result<Vec<Value>, PersistError> {
    let own = handle.knowledge().knowledge_candidate_rows(
        pool.project.id,
        pool.input,
        pool.wanted,
        pool.vector,
    );
    if !pool.reranked || !state.rerank_service().current_config().search_no_project {
        return own.await;
    }
    let (rows, more) = tokio::join!(own, extra_rows(handle, &pool));
    let rows = rows?;
    Ok(match more? {
        Some(more) => merge(rows, more, usize::try_from(pool.wanted).unwrap_or(0)),
        None => rows,
    })
}

async fn extra_rows(
    handle: &PersistHandle,
    pool: &Pool<'_>,
) -> Result<Option<Vec<Value>>, PersistError> {
    let Some(extra) = handle.knowledge().no_project_knowledge_project().await? else {
        return Ok(None);
    };
    if extra.id == pool.project.id {
        return Ok(None);
    }
    let mut rows = handle
        .knowledge()
        .knowledge_candidate_rows(extra.id, pool.input, pool.wanted, pool.vector)
        .await?;
    rows.iter_mut().for_each(mark_outside_project);
    Ok(Some(rows))
}

pub fn merge(own: Vec<Value>, extra: Vec<Value>, wanted: usize) -> Vec<Value> {
    let mut seen = HashSet::new();
    let mut merged: Vec<Value> = own
        .into_iter()
        .chain(extra)
        .filter(|row| seen.insert(reference(row)))
        .collect();
    merged.sort_by(|a, b| {
        identifier_hits(b)
            .cmp(&identifier_hits(a))
            .then_with(|| rank(b).total_cmp(&rank(a)))
    });
    merged.truncate(wanted);
    merged
}

fn reference(row: &Value) -> String {
    row["reference"].as_str().unwrap_or_default().to_owned()
}

fn rank(row: &Value) -> f64 {
    row["rank"].as_f64().unwrap_or(0.0)
}
