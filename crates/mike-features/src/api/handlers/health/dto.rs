use super::*;

#[derive(Serialize)]
pub struct EmbeddingHealth {
    pub knowledge: serde_json::Value,
    pub url: String,
    pub reachable: bool,
    pub reachable_state: ProbeState,
    pub closed_unembedded_turns: Option<i64>,
    pub open_unembedded_turns: Option<i64>,
    pub knowledge_age_secs: Option<i64>,
    pub turns_age_secs: Option<i64>,
}

#[derive(Serialize)]
pub struct PersistenceHealth {
    pub enabled: bool,
    pub reachable: bool,
    pub reachable_state: ProbeState,
    pub capture_probe_state: ProbeState,
    pub latest_capture_write_at: Option<String>,
    pub capture_write_age_secs: Option<i64>,
    pub capture_stalled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

#[derive(Serialize)]
pub struct SteeringHealth {
    pub key_configured: bool,
    pub consecutive_failures: u32,
    pub last_failure_status: Option<u16>,
    pub last_failure_at: Option<String>,
}

#[derive(Serialize)]
pub struct JudgeHealth {
    pub judge_expected: bool,
    pub stale_after_secs: i64,
    pub stale_unjudged_closed_turns: Option<i64>,
    pub stale_unjudged_at_least: bool,
    pub oldest_unjudged_closed_at: Option<String>,
    pub backlog_alarm: bool,
    pub backlog_age_secs: Option<i64>,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub instance: InstanceHealth,
    pub status: &'static str,
    pub embedding: EmbeddingHealth,
    pub rerank: RerankHealth,
    pub persistence: PersistenceHealth,
    pub capture_gap: gap::CaptureGapHealth,
    pub unrecorded_captures: crate::store::persistence::capture_retry::CaptureRetryHealth,
    pub llm: SteeringHealth,
    pub judge: JudgeHealth,
    pub db_cost: Option<crate::db_cost::health::DbCostHealth>,
    pub build: BuildIdentity,
    pub process: ProcessIdentity,
}

#[derive(Serialize)]
pub struct ProcessIdentity {
    pub started_at: String,
    pub uptime_seconds: u64,
}

impl ProcessIdentity {
    pub fn from_uptime(uptime: std::time::Duration) -> Self {
        let started = chrono::Utc::now() - chrono::Duration::from_std(uptime).unwrap_or_default();
        Self {
            started_at: started.to_rfc3339(),
            uptime_seconds: uptime.as_secs(),
        }
    }
}

#[derive(Serialize)]
pub struct BuildIdentity {
    pub sha: &'static str,
    pub sha_short: &'static str,
    pub dirty: bool,
    pub built_at: &'static str,
}

impl BuildIdentity {
    pub fn current() -> Self {
        let build = crate::change_log::current_build();
        Self {
            sha: build.sha,
            sha_short: build.sha_short,
            dirty: build.dirty,
            built_at: build.built_at,
        }
    }
}

#[derive(Serialize)]
pub struct InstanceHealth {
    pub instance_id: String,
    pub boot_generation: String,
}
