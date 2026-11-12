use serde::Serialize;

use crate::analysis::rerank::{RerankPhase, RerankStatus};
use crate::semantic::egress::{EgressState, egress_state_for_url};
use crate::state::AppState;

const RERANK_SETTINGS_PAGE: &str = "/settings/embedding";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RerankRowState {
    Off,
    Ok,
    Degraded,
    Inert,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RerankHealth {
    pub(crate) enabled: bool,
    pub(crate) state: RerankRowState,
    pub(crate) phase: RerankPhase,
    pub(crate) url: String,
    pub(crate) model: String,
    pub(crate) egress: EgressState,
    pub(crate) breaker_open: bool,
    pub(crate) last_ok_at: Option<String>,
    pub(crate) last_error: Option<String>,
    pub(crate) last_error_at: Option<String>,
    pub(crate) detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fix: Option<String>,
}

pub(crate) fn rerank_health(state: &AppState) -> RerankHealth {
    let status = state.rerank_service().status();
    let allow_remote = state.semantic().config().allow_remote_embedding;
    describe(status, allow_remote)
}

pub(super) fn describe(status: RerankStatus, allow_remote: bool) -> RerankHealth {
    let egress = egress_state_for_url(&status.url, allow_remote);
    let (row, detail, fix) = classify(&status, egress);
    RerankHealth {
        enabled: status.phase != RerankPhase::Off,
        state: row,
        phase: status.phase,
        egress,
        breaker_open: status.breaker_open,
        last_ok_at: status.last_ok_at.map(|at| at.to_rfc3339()),
        last_error: status.last_error,
        last_error_at: status.last_error_at.map(|at| at.to_rfc3339()),
        url: status.url,
        model: status.model,
        detail,
        fix,
    }
}

fn classify(
    status: &RerankStatus,
    egress: EgressState,
) -> (RerankRowState, String, Option<String>) {
    let url = &status.url;
    if status.phase == RerankPhase::Off {
        return (
            RerankRowState::Off,
            "Reranker is disabled via rerank.enabled; search results keep their plain order"
                .to_owned(),
            Some(format!("Enable rerank.enabled at {RERANK_SETTINGS_PAGE}")),
        );
    }
    if egress == EgressState::RemoteBlocked {
        return (
            RerankRowState::Inert,
            format!(
                "Reranker at {url} is not on this machine and remote egress is not allowed, so \
                 searches skip it and keep their plain order"
            ),
            Some(format!(
                "Turn on semantic.allow_remote_embedding at /settings, or point rerank.url at a \
                 local server at {RERANK_SETTINGS_PAGE}"
            )),
        );
    }
    match status.phase {
        RerankPhase::Idle => (
            RerankRowState::Ok,
            format!("Reranker enabled against {url}; no search has used it yet"),
            None,
        ),
        RerankPhase::Ready => (
            RerankRowState::Ok,
            format!("Reranker active against the server at {url}"),
            None,
        ),
        RerankPhase::Loading => (
            RerankRowState::Ok,
            format!(
                "Reranker at {url} is loading its model; searches keep their plain order until \
                 it is ready"
            ),
            None,
        ),
        RerankPhase::Unavailable | RerankPhase::Off => unavailable(status),
    }
}

fn unavailable(status: &RerankStatus) -> (RerankRowState, String, Option<String>) {
    let url = &status.url;
    let why = status
        .last_error
        .as_deref()
        .unwrap_or("repeated failures opened the circuit breaker");
    let retry = if status.breaker_open {
        "; calls are paused until the breaker retries"
    } else {
        ""
    };
    (
        RerankRowState::Degraded,
        format!("Reranker at {url} did not answer: {why}{retry}. Searches keep their plain order"),
        Some(format!(
            "Check that the reranker server is running at {url} (on the machine that hosts it: \
             systemctl --user status mike-stack-rerank), or turn rerank.enabled off at \
             {RERANK_SETTINGS_PAGE}"
        )),
    )
}
