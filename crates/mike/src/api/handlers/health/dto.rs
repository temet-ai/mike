use super::*;

#[derive(Serialize)]
pub(crate) struct EmbeddingHealth {
    pub(crate) knowledge: serde_json::Value,
    pub(crate) url: String,
    pub(crate) reachable: bool,
    pub(crate) reachable_state: ProbeState,
    pub(crate) closed_unembedded_turns: Option<i64>,
    pub(crate) open_unembedded_turns: Option<i64>,
    pub(crate) knowledge_age_secs: Option<i64>,
    pub(crate) turns_age_secs: Option<i64>,
}

#[derive(Serialize)]
pub(crate) struct PersistenceHealth {
    pub(crate) enabled: bool,
    pub(crate) reachable: bool,
    pub(crate) reachable_state: ProbeState,
    pub(crate) capture_probe_state: ProbeState,
    pub(crate) latest_capture_write_at: Option<String>,
    pub(crate) capture_write_age_secs: Option<i64>,
    pub(crate) capture_stalled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fix: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct SteeringHealth {
    pub(crate) key_configured: bool,
    pub(crate) consecutive_failures: u32,
    pub(crate) last_failure_status: Option<u16>,
    pub(crate) last_failure_at: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct JudgeHealth {
    pub(crate) judge_expected: bool,
    pub(crate) stale_after_secs: i64,
    pub(crate) stale_unjudged_closed_turns: Option<i64>,
    pub(crate) stale_unjudged_at_least: bool,
    pub(crate) oldest_unjudged_closed_at: Option<String>,
    pub(crate) backlog_alarm: bool,
    pub(crate) backlog_age_secs: Option<i64>,
}

#[derive(Serialize)]
pub(crate) struct HealthResponse {
    pub(crate) instance: InstanceHealth,
    pub(crate) status: &'static str,
    pub(crate) embedding: EmbeddingHealth,
    pub(crate) rerank: RerankHealth,
    pub(crate) persistence: PersistenceHealth,
    pub(crate) capture_gap: gap::CaptureGapHealth,
    pub(crate) unrecorded_captures: crate::store::persistence::capture_retry::CaptureRetryHealth,
    pub(crate) llm: SteeringHealth,
    pub(crate) judge: JudgeHealth,
    pub(crate) db_cost: Option<crate::db_cost::health::DbCostHealth>,
    pub(crate) build: BuildIdentity,
    pub(crate) process: ProcessIdentity,
}

#[derive(Serialize)]
pub(crate) struct ProcessIdentity {
    pub(crate) started_at: String,
    pub(crate) uptime_seconds: u64,
}

impl ProcessIdentity {
    pub(crate) fn from_uptime(uptime: std::time::Duration) -> Self {
        let started = chrono::Utc::now() - chrono::Duration::from_std(uptime).unwrap_or_default();
        Self {
            started_at: started.to_rfc3339(),
            uptime_seconds: uptime.as_secs(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct BuildIdentity {
    pub(crate) sha: &'static str,
    pub(crate) sha_short: &'static str,
    pub(crate) dirty: bool,
    pub(crate) built_at: &'static str,
}

impl BuildIdentity {
    pub(crate) fn current() -> Self {
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
pub(crate) struct InstanceHealth {
    pub(crate) instance_id: String,
    pub(crate) boot_generation: String,
}
