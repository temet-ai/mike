use chrono::Utc;
use uuid::Uuid;

use super::block::Block;
use super::record::spawn_record;
use super::served::mark_served;
use super::supply::{Carrier, SupplyRequest, supply};
use crate::state::AppState;

const RECENT_PROMPTS: usize = 3;

pub(crate) struct CardAsk<'a> {
    pub(crate) session: Uuid,
    pub(crate) project_path: &'a str,
    pub(crate) prompt: Option<&'a str>,
    pub(crate) cwd: Option<&'a str>,
    pub(crate) carrier: Carrier,
}

pub(crate) fn serves_cards(state: &AppState) -> bool {
    crate::feature_toggles::client_serves(state, "client_serves_memory_keeper")
}

pub(crate) async fn card_for(state: &AppState, ask: &CardAsk<'_>) -> Option<Block> {
    let recent = recent_prompts(state, &ask.session.to_string()).await;
    let snapshot = state.keeper_snapshot.load();
    let outcome = supply(
        &snapshot,
        &SupplyRequest {
            session: ask.session,
            project_path: ask.project_path,
            prompt: ask.prompt,
            cwd: ask.cwd,
            branch: None,
            recent: recent.iter().map(String::as_str).collect(),
            carrier: ask.carrier,
            now: Utc::now(),
        },
    );
    if let Some(record) = outcome.record {
        spawn_record(state.persist().cloned(), record);
    }
    let block = outcome.block?;
    mark_served(&ask.session.to_string());
    Some(block)
}

async fn recent_prompts(state: &AppState, session_id: &str) -> Vec<String> {
    let guard = state.store.read().await;
    let ids = guard.requests_by_session_id(session_id);
    ids.iter()
        .rev()
        .filter_map(|id| guard.get(id))
        .map(|request| request.human_prompt.clone())
        .filter(|prompt| !prompt.trim().is_empty())
        .take(RECENT_PROMPTS)
        .collect()
}
