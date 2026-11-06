use std::collections::HashSet;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use uuid::Uuid;

use super::own_marker::{own_channel_ids, raw_marker_ids};
use crate::proxy::remote_control::route::is_channel_id;
use crate::store::persistence::PersistHandle;

const UNTHROTTLED_SCANS: u8 = 20;
const RESCAN_INTERVAL: Duration = Duration::from_secs(60);
const MAX_SEEN_IDS: usize = 256;

#[derive(Debug, Default)]
pub struct LinkProgress {
    scans: u8,
    last_scan: Option<Instant>,
    seen: HashSet<String>,
}

pub type ChannelLinkProgress = DashMap<String, LinkProgress>;

pub fn claim_scan(progress: &ChannelLinkProgress, session_id: &str, now: Instant) -> bool {
    let mut entry = progress.entry(session_id.to_owned()).or_default();
    let due = entry.scans < UNTHROTTLED_SCANS
        || entry
            .last_scan
            .is_none_or(|at| now.saturating_duration_since(at) >= RESCAN_INTERVAL);
    if due {
        entry.scans = entry.scans.saturating_add(1);
        entry.last_scan = Some(now);
    }
    due
}

pub fn unseen_channels(
    progress: &ChannelLinkProgress,
    session_id: &str,
    body: &[u8],
) -> Vec<String> {
    let raw = raw_marker_ids(body);
    let fresh: Vec<&str> = {
        let entry = progress.entry(session_id.to_owned()).or_default();
        raw.iter()
            .copied()
            .filter(|id| !entry.seen.contains(*id))
            .collect()
    };
    if fresh.is_empty() {
        return Vec::new();
    }
    let own = own_channel_ids(body);
    let mut entry = progress.entry(session_id.to_owned()).or_default();
    if entry.seen.len() + fresh.len() > MAX_SEEN_IDS {
        entry.seen.clear();
    }
    for id in &fresh {
        entry.seen.insert((*id).to_owned());
    }
    own.into_iter()
        .filter(|id| fresh.contains(&id.as_str()))
        .map(|id| format!("cse_{id}"))
        .filter(|cse_id| is_channel_id(cse_id))
        .collect()
}

pub async fn link_session_channels(
    persist: &PersistHandle,
    progress: &ChannelLinkProgress,
    session_uuid: Uuid,
    body: &[u8],
) -> usize {
    let cse_ids = unseen_channels(progress, &session_uuid.to_string(), body);
    write_links(persist, progress, session_uuid, &cse_ids).await
}

pub async fn write_links(
    persist: &PersistHandle,
    progress: &ChannelLinkProgress,
    session_uuid: Uuid,
    cse_ids: &[String],
) -> usize {
    let session_id = session_uuid.to_string();
    let mut linked = 0;
    for cse_id in cse_ids {
        match persist
            .link_channel_from_request_body(cse_id, session_uuid)
            .await
        {
            Ok(()) => linked += 1,
            Err(err) => {
                if let Some(mut entry) = progress.get_mut(&session_id) {
                    entry.seen.remove(cse_id.trim_start_matches("cse_"));
                }
                tracing::warn!(
                    cse_id = %cse_id,
                    session_uuid = %session_uuid,
                    error = %err,
                    "failed to link remote control channel from request body"
                );
            }
        }
    }
    linked
}

#[cfg(test)]
mod tests {
    use super::{ChannelLinkProgress, UNTHROTTLED_SCANS, claim_scan, unseen_channels};
    use serde_json::json;
    use std::time::{Duration, Instant};

    fn reminder(id: &str) -> String {
        format!(
            "<system-reminder>\nAttribution for git commits and pull requests\nClaude-Session: https://claude.ai/code/session_{id}\n</system-reminder>"
        )
    }

    fn body(own: &[&str], tool_output: &str) -> Vec<u8> {
        let mut messages: Vec<serde_json::Value> = own
            .iter()
            .map(|id| json!({"role":"user","content":[{"type":"text","text":reminder(id)}]}))
            .collect();
        messages.push(json!({"role":"user","content":[{"type":"tool_result",
            "tool_use_id":"t","content":tool_output}]}));
        json!({"messages": messages}).to_string().into_bytes()
    }

    #[test]
    fn claim_scan_is_open_for_the_first_scans_then_waits_out_the_interval() {
        let progress = ChannelLinkProgress::new();
        let start = Instant::now();
        for _ in 0..UNTHROTTLED_SCANS {
            assert!(claim_scan(&progress, "s", start));
        }
        assert!(!claim_scan(&progress, "s", start + Duration::from_secs(1)));
        assert!(claim_scan(&progress, "s", start + Duration::from_secs(61)));
    }

    #[test]
    fn unseen_channels_returns_each_own_channel_once() {
        let progress = ChannelLinkProgress::new();
        let b = body(&["OLD111", "NEW222"], "x claude.ai/code/session_FOREIGN9");
        assert_eq!(
            unseen_channels(&progress, "s", &b),
            vec!["cse_OLD111", "cse_NEW222"]
        );
        assert!(unseen_channels(&progress, "s", &b).is_empty());
    }

    #[test]
    fn unseen_channels_picks_up_a_new_channel_after_the_old_one_is_known() {
        let progress = ChannelLinkProgress::new();
        unseen_channels(&progress, "s", &body(&["OLD111"], ""));
        assert_eq!(
            unseen_channels(&progress, "s", &body(&["OLD111", "NEW222"], "")),
            vec!["cse_NEW222"]
        );
    }
}
