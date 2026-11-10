use serde_json::json;
use uuid::Uuid;

use super::arm::{KeeperArm, arm_for};
use super::block::Block;
use super::earlier::{Hit, hits_from, pick, points_at_past, render, search_query};
use super::supply::{Carrier, SupplyRecord};
use crate::api::project_knowledge::retrieve;
use crate::config::AppState;
use crate::store::persistence::project_knowledge::KnowledgeQuery;

pub(crate) const CATCH_ALL_PROJECT: &str = "(no project)";
const CANDIDATES: i64 = 20;

pub(crate) struct EarlierAsk<'a> {
    pub(crate) session: Uuid,
    pub(crate) project_path: &'a str,
    pub(crate) prompt: &'a str,
    pub(crate) carrier: Carrier,
}

pub(crate) struct EarlierSupply {
    pub(crate) block: Option<Block>,
    pub(crate) record: SupplyRecord,
}

pub(crate) async fn earlier_work(state: &AppState, ask: &EarlierAsk<'_>) -> Option<EarlierSupply> {
    if !points_at_past(ask.prompt) {
        return None;
    }
    let query = search_query(ask.prompt)?;
    let hits = search_projects(state, ask.project_path, &query).await;
    let picked = pick(hits, ask.session);
    let arm = arm_for(&ask.session.to_string());
    let rendered = render(&picked);
    let record = SupplyRecord {
        session: ask.session,
        thread_slug: None,
        thread_id: None,
        content_hash: rendered.as_ref().map(|b| b.hash.clone()),
        carrier: ask.carrier,
        arm,
        resolution: "earlier",
        signals: json!({
            "query": query,
            "refs": picked.iter().map(|h| h.reference.as_str()).collect::<Vec<_>>(),
        }),
    };
    Some(EarlierSupply {
        block: rendered.filter(|_| arm == KeeperArm::Keeper),
        record,
    })
}

async fn search_projects(state: &AppState, project_path: &str, query: &str) -> Vec<Hit> {
    let own = search_one(state, project_path, query);
    if project_path == CATCH_ALL_PROJECT {
        return own.await;
    }
    let (mut hits, catch_all) = tokio::join!(own, search_one(state, CATCH_ALL_PROJECT, query));
    hits.extend(catch_all);
    hits
}

async fn search_one(state: &AppState, project: &str, query: &str) -> Vec<Hit> {
    let input = KnowledgeQuery {
        project: project.to_owned(),
        query: query.to_owned(),
        limit: Some(CANDIDATES),
        ..KnowledgeQuery::default()
    };
    match retrieve(state, &input).await {
        Ok(page) => hits_from(&page),
        Err(_) => Vec::new(),
    }
}
