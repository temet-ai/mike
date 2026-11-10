use std::collections::HashSet;

use serde_json::Value;

use crate::config::AppState;
use crate::store::persistence::project_knowledge::{KnowledgeQuery, ProjectIdentity};
use crate::store::persistence::{PersistError, PersistHandle};

pub(super) const NO_PROJECT_PATH: &str = "(no project)";

pub(super) struct Pool<'a> {
    pub(super) project: &'a ProjectIdentity,
    pub(super) input: &'a KnowledgeQuery,
    pub(super) wanted: i64,
    pub(super) vector: Option<&'a (String, String, String)>,
    pub(super) reranked: bool,
}

pub(super) async fn candidate_rows(
    state: &AppState,
    handle: &PersistHandle,
    pool: Pool<'_>,
) -> Result<Vec<Value>, PersistError> {
    let rows = handle
        .knowledge_candidate_rows(pool.project.id, pool.input, pool.wanted, pool.vector)
        .await?;
    if !pool.reranked || !state.rerank_service().current_config().search_no_project {
        return Ok(rows);
    }
    let Some(extra) = handle.find_knowledge_project(NO_PROJECT_PATH).await? else {
        return Ok(rows);
    };
    if extra.id == pool.project.id {
        return Ok(rows);
    }
    let more = handle
        .knowledge_candidate_rows(extra.id, pool.input, pool.wanted, pool.vector)
        .await?;
    Ok(merge(rows, more, usize::try_from(pool.wanted).unwrap_or(0)))
}

pub(super) fn merge(own: Vec<Value>, extra: Vec<Value>, wanted: usize) -> Vec<Value> {
    let mut seen = HashSet::new();
    let mut merged: Vec<Value> = own
        .into_iter()
        .chain(extra)
        .filter(|row| seen.insert(reference(row)))
        .collect();
    merged.sort_by(|a, b| rank(b).total_cmp(&rank(a)));
    merged.truncate(wanted);
    merged
}

fn reference(row: &Value) -> String {
    row["reference"].as_str().unwrap_or_default().to_owned()
}

fn rank(row: &Value) -> f64 {
    row["rank"].as_f64().unwrap_or(0.0)
}
