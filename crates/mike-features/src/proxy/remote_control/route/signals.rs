use serde::Deserialize;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChannelSignals {
    pub client_session_id: Option<String>,
    pub worker_epoch: Option<i64>,
    pub worker_status: Option<String>,
    pub idle_seconds: Option<i32>,
    pub current_interval_seconds: Option<i32>,
    pub is_compaction: Option<bool>,
    pub session_agent_id: Option<String>,
    pub preserved_event_ids: Option<Vec<String>>,
    pub post_turn_summary: Option<serde_json::Value>,
    pub local_session_uuid: Option<String>,
    pub local_cwd: Option<String>,
    pub client_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SignalPayload {
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    worker_epoch: Option<i64>,
    #[serde(default)]
    worker_status: Option<String>,
    #[serde(default)]
    idle_seconds: Option<i32>,
    #[serde(default)]
    current_interval_seconds: Option<i32>,
    #[serde(default)]
    is_compaction: Option<bool>,
    #[serde(default)]
    session_agent_id: Option<String>,
    #[serde(default)]
    preserved_event_ids: Option<Vec<String>>,
    #[serde(default)]
    external_metadata: Option<ExternalMetadata>,
}

#[derive(Debug, Deserialize)]
struct ExternalMetadata {
    #[serde(default)]
    post_turn_summary: Option<serde_json::Value>,
}

pub fn parse_channel_signals(body: &[u8]) -> ChannelSignals {
    let Ok(payload) = serde_json::from_slice::<SignalPayload>(body) else {
        return ChannelSignals::default();
    };
    ChannelSignals {
        client_session_id: payload.session_id,
        worker_epoch: payload.worker_epoch,
        worker_status: payload.worker_status,
        idle_seconds: payload.idle_seconds,
        current_interval_seconds: payload.current_interval_seconds,
        is_compaction: payload.is_compaction,
        session_agent_id: payload.session_agent_id,
        preserved_event_ids: payload.preserved_event_ids,
        post_turn_summary: payload.external_metadata.and_then(|m| m.post_turn_summary),
        local_session_uuid: None,
        local_cwd: None,
        client_version: None,
    }
}
