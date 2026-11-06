use std::time::Instant;

use uuid::Uuid;

use crate::channel_link_sweep::link::{claim_scan, unseen_channels, write_links};
use crate::config::AppState;

pub(crate) fn maybe_link_channel(state: &AppState, session_id: &str, body: &[u8]) {
    if state.persist().is_none() {
        return;
    }
    let Ok(session_uuid) = Uuid::parse_str(session_id) else {
        return;
    };
    if !claim_scan(&state.channel_link_attempted, session_id, Instant::now()) {
        return;
    }
    let cse_ids = unseen_channels(&state.channel_link_attempted, session_id, body);
    if cse_ids.is_empty() {
        return;
    }

    let state = state.clone();
    let tracker = state.tracker.clone();
    tracker.spawn(async move {
        let Some(persist) = state.persist() else {
            return;
        };
        write_links(
            persist,
            &state.channel_link_attempted,
            session_uuid,
            &cse_ids,
        )
        .await;
    });
}
