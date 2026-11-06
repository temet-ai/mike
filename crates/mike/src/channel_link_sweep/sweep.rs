use super::own_marker::own_channel_ids;
use crate::proxy::remote_control::route::is_channel_id;
use crate::store::persistence::remote_control::ChannelLinkSweepCandidate;
use crate::store::persistence::{PersistError, PersistHandle};

pub async fn run_sweep_tick(persist: &PersistHandle, window_minutes: i64, batch_size: i64) {
    let since = chrono::Utc::now() - chrono::Duration::minutes(window_minutes);
    let candidates = match persist
        .fetch_channel_link_sweep_candidates(since, batch_size)
        .await
    {
        Ok(candidates) => candidates,
        Err(e) => {
            log_candidate_query_failed(&e);
            return;
        }
    };

    let considered = candidates.len();
    let linked = link_all_candidates(persist, &candidates).await;
    log_tick_complete(considered, linked);
}

fn log_candidate_query_failed(e: &PersistError) {
    tracing::warn!(error = %e, "channel_link_sweep: candidate query failed");
}

async fn link_all_candidates(
    persist: &PersistHandle,
    candidates: &[ChannelLinkSweepCandidate],
) -> usize {
    let mut linked = 0usize;
    for candidate in candidates {
        if link_candidate(persist, candidate).await {
            linked += 1;
        }
    }
    linked
}

fn log_tick_complete(considered: usize, linked: usize) {
    log_tick_debug(considered, linked);
    if linked > 0 {
        log_tick_linked(considered, linked);
    }
}

fn log_tick_debug(considered: usize, linked: usize) {
    tracing::debug!(considered, linked, "channel_link_sweep: tick complete");
}

fn log_tick_linked(considered: usize, linked: usize) {
    tracing::info!(
        considered,
        linked,
        "channel_link_sweep: linked channels via periodic sweep"
    );
}

fn candidate_cse_ids(body: &[u8]) -> Vec<String> {
    own_channel_ids(body)
        .into_iter()
        .map(|id| format!("cse_{id}"))
        .filter(|cse_id| is_channel_id(cse_id))
        .collect()
}

async fn link_candidate(persist: &PersistHandle, candidate: &ChannelLinkSweepCandidate) -> bool {
    let Some(body) = candidate.request_body.as_deref() else {
        return false;
    };
    let mut linked = false;
    for cse_id in candidate_cse_ids(body) {
        match persist
            .link_channel_from_request_body(&cse_id, candidate.session_uuid)
            .await
        {
            Ok(()) => linked = true,
            Err(e) => {
                tracing::warn!(
                    cse_id = %cse_id,
                    session_uuid = %candidate.session_uuid,
                    error = %e,
                    "channel_link_sweep: link write failed"
                );
            }
        }
    }
    linked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_cse_ids_reads_the_attribution_reminder() {
        let body = serde_json::json!({"messages":[{"role":"user","content":[{"type":"text",
            "text":"<system-reminder>\nAttribution for git commits and pull requests\nClaude-Session: https://claude.ai/code/session_01SAMPLE\n</system-reminder>"}]}]})
        .to_string();
        assert_eq!(
            candidate_cse_ids(body.as_bytes()),
            vec!["cse_013wPVgfQVUPDXraeMwjytZq".to_owned()]
        );
    }

    #[test]
    fn candidate_cse_ids_no_marker_returns_nothing() {
        assert!(candidate_cse_ids(b"no marker in this body at all").is_empty());
    }
}
