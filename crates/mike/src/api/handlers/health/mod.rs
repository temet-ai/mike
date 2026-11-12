use super::*;

pub(crate) mod capture_outage;
mod dto;
mod gap;
mod judge;
mod probe;
mod rerank;
#[cfg(test)]
mod rerank_tests;
mod stored;
#[cfg(test)]
mod unrecorded_captures_tests;

pub(crate) use dto::*;
pub(crate) use probe::ProbeState;
pub(crate) use rerank::{RerankHealth, RerankRowState, rerank_health};
pub(crate) use stored::spawn_health_refresher;

#[cfg(any(test, feature = "test-seams"))]
pub async fn refresh_health_now(state: &AppState) {
    stored::refresh_once(state).await;
}

#[cfg(any(test, feature = "test-seams"))]
pub async fn refresh_capture_outage_now(state: &AppState) {
    stored::note_capture_outage(state).await;
}

#[cfg(any(test, feature = "test-seams"))]
pub fn capture_outage_notices() -> std::sync::mpsc::Receiver<String> {
    capture_outage::install_test_sink()
}

pub(crate) const CAPTURE_STALL_TRAFFIC_WINDOW_SECS: i64 = 120;

pub(crate) const CAPTURE_STALL_WRITE_AGE_SECS: i64 = 120;
pub(crate) const CAPTURE_STALL_STARTUP_GRACE_SECS: u64 = 30;

pub(crate) async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let stored = stored::current();
    let now = chrono::Utc::now();
    let service = state.embedding_service();
    let (embedding_state, persistence_state, capture) = tokio::join!(
        probe::probe_embedding(&service, probe::LIVE_PROBE_DEADLINE),
        probe::probe_persistence(state.persist(), probe::LIVE_PROBE_DEADLINE),
        probe::probe_latest_capture(state.persist(), probe::LIVE_PROBE_DEADLINE),
    );
    let embedding = embedding_health(&state, &stored, embedding_state, now);
    let persistence = persistence_health(&state, persistence_state, capture);
    let llm = steering_health(&state);
    let llm_failures_degrade_status = state
        .settings_overlay
        .load()
        .health_llm_failures_degrade_status;
    let status = compute_health_status(
        SubsystemReadiness::new(persistence.enabled, persistence.reachable),
        SubsystemReadiness::new(
            state.embedding_service().is_available(),
            embedding.reachable,
        ),
        llm.consecutive_failures,
        llm_failures_degrade_status,
        persistence.capture_stalled,
    );
    let judge = judge::judge_health(&state);
    let status = judge::apply_judge_alarm(status, judge.backlog_alarm);
    let capture_gap = gap::current(&stored, now);
    let status = gap::apply_gap_alarm(status, capture_gap.alarm);
    let db_cost_report = crate::db_cost::current();
    let status = crate::db_cost::health::apply_alarm(status, db_cost_report.as_deref());
    let db_cost = crate::db_cost::health::health_section(db_cost_report.as_deref());

    Json(HealthResponse {
        instance: InstanceHealth {
            instance_id: state.config().instance_id.clone(),
            boot_generation: state.config().boot_generation.clone(),
        },
        status,
        embedding,
        rerank: rerank_health(&state),
        persistence,
        capture_gap,
        unrecorded_captures: crate::store::persistence::capture_retry::health(),
        llm,
        judge,
        db_cost,
        build: BuildIdentity::current(),
        process: ProcessIdentity::from_uptime(state.started_at.elapsed()),
    })
}

fn embedding_health(
    state: &AppState,
    stored: &stored::StoredSections,
    reachable_state: ProbeState,
    now: chrono::DateTime<chrono::Utc>,
) -> EmbeddingHealth {
    let turns = stored.turns.as_ref();
    EmbeddingHealth {
        knowledge: stored
            .knowledge
            .as_ref()
            .map_or(serde_json::Value::Null, |section| section.value.clone()),
        url: state.embedding_service().url(),
        reachable: reachable_state == ProbeState::Ok,
        reachable_state,
        closed_unembedded_turns: turns.and_then(|section| section.value.closed_unembedded),
        open_unembedded_turns: turns.and_then(|section| section.value.open_unembedded),
        knowledge_age_secs: stored
            .knowledge
            .as_ref()
            .map(|section| section.age_secs(now)),
        turns_age_secs: turns.map(|section| section.age_secs(now)),
    }
}

fn persistence_health(
    state: &AppState,
    reachable_state: ProbeState,
    capture: (ProbeState, Option<chrono::DateTime<chrono::Utc>>),
) -> PersistenceHealth {
    let enabled = state.persist().is_some();
    let (capture_probe_state, newest) = capture;
    let capture_write_age_secs = newest.map(|at| (chrono::Utc::now() - at).num_seconds().max(0));
    let capture_stalled = compute_capture_stalled(
        enabled,
        state.last_capturable_request_at(),
        capture_write_age_secs,
        state.started_at.elapsed().as_secs(),
        chrono::Utc::now(),
    );
    PersistenceHealth {
        enabled,
        reachable: reachable_state == ProbeState::Ok,
        reachable_state,
        capture_probe_state,
        latest_capture_write_at: newest.map(|at| at.to_rfc3339()),
        capture_write_age_secs,
        capture_stalled,
        fix: capture_stall_fix(capture_stalled, capture_write_age_secs),
    }
}

pub(crate) fn capture_stall_fix(
    stalled: bool,
    capture_write_age_secs: Option<i64>,
) -> Option<String> {
    if !stalled {
        return None;
    }
    let silence = match capture_write_age_secs {
        Some(age) => format!("the newest captured request is {age}s old"),
        None => "no request has been captured since the proxy started".to_owned(),
    };
    Some(format!(
        "Requests that must be captured reached the proxy in the last \
         {CAPTURE_STALL_TRAFFIC_WINDOW_SECS}s but {silence}: the capture write path is \
         stalled. Check .deploy/proxy.log for persistence errors and /api/db-cost for a \
         saturated pool, then redeploy with ./scripts/start-proxy.sh"
    ))
}

fn steering_health(state: &AppState) -> SteeringHealth {
    use secrecy::ExposeSecret;
    let health = crate::llm::health::snapshot();
    let (last_failure_status, last_failure_at) = health
        .last_failure
        .as_ref()
        .map(|failure| (failure.status, Some(failure.at.to_rfc3339())))
        .unwrap_or_default();
    SteeringHealth {
        key_configured: state
            .openrouter_key()
            .is_some_and(|key| !key.expose_secret().is_empty()),
        consecutive_failures: health.consecutive_failures,
        last_failure_status,
        last_failure_at,
    }
}

pub(crate) async fn probe_embedding_reachable(embedding_service: &EmbeddingService) -> bool {
    probe::probe_embedding_url(embedding_service, probe::CAPABILITY_PROBE_DEADLINE).await
        == ProbeState::Ok
}

pub(crate) async fn probe_persistence_reachable(
    persist: &crate::store::persistence::PersistHandle,
) -> bool {
    probe::probe_persistence(Some(persist), probe::CAPABILITY_PROBE_DEADLINE).await
        == ProbeState::Ok
}

pub(crate) fn compute_capture_stalled(
    persistence_enabled: bool,
    last_capturable_request_at: Option<chrono::DateTime<chrono::Utc>>,
    capture_write_age_secs: Option<i64>,
    uptime_secs: u64,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    if !persistence_enabled {
        return false;
    }
    let traffic_recent = last_capturable_request_at
        .is_some_and(|t| (now - t).num_seconds() <= CAPTURE_STALL_TRAFFIC_WINDOW_SECS);
    if !traffic_recent {
        return false;
    }
    match capture_write_age_secs {
        Some(age) => age > CAPTURE_STALL_WRITE_AGE_SECS,
        None => uptime_secs > CAPTURE_STALL_STARTUP_GRACE_SECS,
    }
}

pub(crate) fn compute_health_status(
    persistence: SubsystemReadiness,
    embedding: SubsystemReadiness,
    llm_consecutive_failures: u32,
    llm_failures_degrade_status: bool,
    capture_stalled: bool,
) -> &'static str {
    let llm_ok = !llm_failures_degrade_status || llm_consecutive_failures == 0;
    if persistence.is_ok() && embedding.is_ok() && llm_ok && !capture_stalled {
        "ok"
    } else {
        "degraded"
    }
}

pub(crate) struct SubsystemReadiness {
    enabled: bool,
    reachable: bool,
}

impl SubsystemReadiness {
    pub(crate) fn new(enabled: bool, reachable: bool) -> Self {
        Self { enabled, reachable }
    }

    fn is_ok(&self) -> bool {
        !self.enabled || self.reachable
    }
}

pub(crate) fn is_permanent_steering_failure(status: Option<u16>) -> bool {
    matches!(status, Some(401) | Some(402) | Some(403))
}

pub(crate) fn describe_steering_failure(
    detail: &crate::llm::health::LlmFailureDetail,
) -> (String, String) {
    match detail.status {
        Some(402) => (
            "the last evaluation call returned HTTP 402 (payment required) — the OpenRouter \
             account is out of credit"
                .to_owned(),
            "OpenRouter key has no credit — top up the account or rotate the key at \
             /settings/integrations"
                .to_owned(),
        ),
        Some(status @ (401 | 403)) => (
            format!(
                "the last evaluation call returned HTTP {status} — the OpenRouter key was \
                 rejected"
            ),
            "OpenRouter key is invalid or revoked — rotate it at /settings/integrations".to_owned(),
        ),
        Some(status) => (
            format!("the last evaluation call returned HTTP {status}"),
            "Check the OpenRouter key at /settings/integrations and the steerer model \
             at /settings/steering"
                .to_owned(),
        ),
        None => (
            format!("the last evaluation call failed: {}", detail.message),
            "Check OpenRouter connectivity and the steerer model at /settings/steering".to_owned(),
        ),
    }
}
