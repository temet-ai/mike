use chrono::{DateTime, Utc};
use serde_json::json;
use uuid::Uuid;

use super::arm::{KeeperArm, arm_for};
use super::block::Block;
use super::earlier::{Hit, MAX_SESSIONS, flatten, points_at_past, render, window};
use super::supply::{Carrier, SupplyRecord};
use crate::api::project_knowledge::retrieval::query_vector;
use crate::config::AppState;
use crate::embed_worker::knowledge::model_key;
use crate::store::persistence::keeper_earlier::{EarlierQuery, EarlierTurn};

pub(crate) const CATCH_ALL_PROJECT: &str = "(no project)";
const MIN_SIMILARITY: f64 = 0.35;
const QUERY_CHARS: usize = 600;
const EXCERPT_CHARS: usize = 160;

pub(crate) struct EarlierAsk<'a> {
    pub(crate) session: Uuid,
    pub(crate) project_path: &'a str,
    pub(crate) prompt: &'a str,
    pub(crate) carrier: Carrier,
    pub(crate) now: DateTime<Utc>,
}

pub(crate) struct EarlierSupply {
    pub(crate) block: Option<Block>,
    pub(crate) record: SupplyRecord,
}

pub(crate) async fn earlier_work(state: &AppState, ask: &EarlierAsk<'_>) -> Option<EarlierSupply> {
    if !points_at_past(ask.prompt) {
        return None;
    }
    let persist = state.persist()?;
    let service = state.embedding_service();
    if !service.is_available() {
        return None;
    }
    let text: String = ask.prompt.chars().take(QUERY_CHARS).collect();
    let (literal, _, model) = query_vector(&service, &model_key(&service), &text).await?;
    let span = window(ask.prompt);
    let paths = [ask.project_path.to_owned(), CATCH_ALL_PROJECT.to_owned()];
    let turns = persist
        .earlier_sessions(&EarlierQuery {
            project_paths: &paths,
            vector_literal: &literal,
            model: &model,
            from: ask.now - span.end,
            to: ask.now - span.start,
            exclude_session: ask.session,
            min_similarity: MIN_SIMILARITY,
            limit: MAX_SESSIONS as i64,
        })
        .await
        .ok()?;
    Some(supply_for(ask, turns.into_iter().map(hit).collect()))
}

fn supply_for(ask: &EarlierAsk<'_>, hits: Vec<Hit>) -> EarlierSupply {
    let arm = arm_for(&ask.session.to_string());
    let rendered = render(&hits);
    let record = SupplyRecord {
        session: ask.session,
        thread_slug: None,
        thread_id: None,
        content_hash: rendered.as_ref().map(|b| b.hash.clone()),
        carrier: ask.carrier,
        arm,
        resolution: "earlier",
        signals: json!({
            "refs": hits.iter().map(|h| h.reference.as_str()).collect::<Vec<_>>(),
            "scores": hits.iter().map(|h| (h.score * 1000.0).round() / 1000.0).collect::<Vec<_>>(),
        }),
    };
    EarlierSupply {
        block: rendered.filter(|_| arm == KeeperArm::Keeper),
        record,
    }
}

fn hit(turn: EarlierTurn) -> Hit {
    Hit {
        reference: format!("turn:{}", turn.turn_id),
        session: turn.session_uuid,
        session_title: turn.session_title.unwrap_or_default(),
        date: turn.created_at.format("%Y-%m-%d").to_string(),
        excerpt: flatten(&turn.prompt, EXCERPT_CHARS),
        score: turn.similarity,
    }
}
