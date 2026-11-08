use std::sync::LazyLock;
use std::time::Duration;

use dashmap::DashSet;

use crate::config::AppState;
use crate::feature_toggles::ResolvedToggles;
use crate::store::models::{RequestId, is_fallback};
use crate::store::persistence::PersistHandle;

const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InterceptOutcome {
    WeaselCorrection,
    ProxyToolAnswer,
    NotScannedBuffered,
}

impl InterceptOutcome {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::WeaselCorrection => "weasel_correction",
            Self::ProxyToolAnswer => "proxy_tool_answer",
            Self::NotScannedBuffered => "not_scanned_buffered",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct InterceptEvidence {
    session_id: String,
    request_id: RequestId,
    outcome: InterceptOutcome,
    subject: Option<String>,
}

impl InterceptEvidence {
    pub(crate) fn new(
        session_id: &str,
        request_id: RequestId,
        outcome: InterceptOutcome,
        subject: Option<String>,
    ) -> Self {
        Self {
            session_id: session_id.to_owned(),
            request_id,
            outcome,
            subject,
        }
    }
}

static GUARDED_BY_CLIENT: LazyLock<DashSet<String>> = LazyLock::new(DashSet::new);

pub(crate) fn mark_guarded_by_client(session_id: &str) {
    GUARDED_BY_CLIENT.insert(session_id.to_owned());
}

pub(crate) fn guarded_by_client(session_id: &str) -> bool {
    GUARDED_BY_CLIENT.contains(session_id)
}

pub(crate) fn weasel_armed(toggles: &ResolvedToggles, session_id: &str) -> bool {
    toggles.weasel_intercept
        && !toggles.weasel_intercept_forbidden_words.is_empty()
        && !guarded_by_client(session_id)
}

pub(crate) fn spawn_record(state: &AppState, evidence: InterceptEvidence) {
    let state = state.clone();
    let tracker = state.tracker.clone();
    crate::observability::spans::spawn_tracked(&tracker, async move {
        record(&state, &evidence).await;
    });
}

pub(crate) fn note_unscanned_buffered(
    state: &AppState,
    toggles: &ResolvedToggles,
    upstream_succeeded: bool,
    session_id: &str,
    request_id: RequestId,
) {
    if upstream_succeeded && weasel_armed(toggles, session_id) {
        spawn_record(
            state,
            InterceptEvidence::new(
                session_id,
                request_id,
                InterceptOutcome::NotScannedBuffered,
                None,
            ),
        );
    }
}

fn warn_not_written(evidence: &InterceptEvidence, reason: &str) {
    tracing::warn!(
        session_id = %evidence.session_id,
        outcome = evidence.outcome.code(),
        reason,
        "response intercept evidence not written"
    );
}

async fn insert_row(
    persist: &PersistHandle,
    session_uuid: uuid::Uuid,
    evidence: &InterceptEvidence,
) -> Result<u64, String> {
    let raw_id = evidence.request_id.as_u64();
    let request_id = (!is_fallback(raw_id)).then_some(raw_id as i64);
    let insert = persist.insert_response_intercept(
        session_uuid,
        request_id,
        evidence.outcome.code(),
        evidence.subject.as_deref(),
    );
    match tokio::time::timeout(WRITE_TIMEOUT, insert).await {
        Ok(Ok(rows)) => Ok(rows),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("write timed out".to_owned()),
    }
}

pub(crate) async fn record(state: &AppState, evidence: &InterceptEvidence) -> bool {
    let Some(persist) = state.persist() else {
        return false;
    };
    let Ok(session_uuid) = uuid::Uuid::parse_str(&evidence.session_id) else {
        warn_not_written(evidence, "session id is not a UUID");
        return false;
    };
    match insert_row(persist, session_uuid, evidence).await {
        Ok(1) => true,
        Ok(_) => {
            warn_not_written(evidence, "session row does not exist");
            false
        }
        Err(reason) => {
            warn_not_written(evidence, &reason);
            false
        }
    }
}
