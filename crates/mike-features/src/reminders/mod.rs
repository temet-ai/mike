pub mod appender;
pub mod change_record_tail;
pub mod client_context;
pub mod code_findings;
pub mod code_findings_push;
pub mod fork_prefix;
pub mod formatter;
pub mod git_read_hint;
pub mod inject;
pub mod lesson_injection_backfill;
pub mod seat_role;
pub mod self_identity;
pub mod stage;
pub mod workspace_briefing;

#[cfg(test)]
pub mod appender_tests;
#[cfg(test)]
pub mod client_context_tests;
#[cfg(test)]
pub mod fork_prefix_tests;
#[cfg(test)]
pub mod git_read_hint_tests;
#[cfg(test)]
pub mod reminders_stage_tests;
#[cfg(test)]
pub mod self_identity_stage_tests;
#[cfg(any(test, feature = "test-seams"))]
pub mod stage_test_support;
#[cfg(any(test, feature = "test-seams"))]
pub mod test_support;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub mod workspace_briefing_stage_tests;

pub use crate::config::reminders::RemindersConfig;
pub use self_identity::{SelfIdentityRequest, apply_self_identity};

use crate::store::RequestStore;
use crate::store::models::{ReminderSource, RequestId, SystemReminder};

#[derive(Debug)]
pub enum RemindersOutcome {
    Unchanged,
    Appended {
        body: Vec<u8>,
        reminder_text: String,
        source_finding_ids: Vec<i64>,
        source_request_id: u64,
    },
    DeliveredOutOfBand {
        reminder_text: String,
        source_finding_ids: Vec<i64>,
        source_request_id: u64,
    },
    Suppressed {
        reason: &'static str,
    },
}

pub struct PushOrAppendRequest<'a> {
    pub push_deps: &'a crate::proxy::remote_control::inject::PushDeps,
    pub session_id: &'a str,
    pub body_bytes: &'a [u8],
    pub source: ReminderSource,
    pub reminder_text: String,
    pub source_finding_ids: Vec<i64>,
    pub source_request_id: u64,
    pub request_id: RequestId,
}

pub async fn push_or_append_reminder(req: PushOrAppendRequest<'_>) -> RemindersOutcome {
    let PushOrAppendRequest {
        push_deps,
        session_id,
        body_bytes,
        source,
        reminder_text,
        source_finding_ids,
        source_request_id,
        request_id,
    } = req;
    let probe = SystemReminder {
        source: source.clone(),
        payload: reminder_text.clone(),
    };
    let admission = push_deps
        .gate
        .admit_for_request(session_id, &probe, body_bytes);
    if let Some(reason) = admission.suppression_reason() {
        tracing::debug!(
            session_id,
            reason,
            source = ?probe.source,
            payload_len = probe.payload.len(),
            "injection gate suppressed a reminder this session has already received"
        );
        return RemindersOutcome::Suppressed { reason };
    }
    if admission.may_wake()
        && let Ok(recipient) = session_id.parse::<uuid::Uuid>()
        && crate::proxy::remote_control::inject::try_push_reminder(push_deps, recipient, &probe)
            .await
    {
        return RemindersOutcome::DeliveredOutOfBand {
            reminder_text,
            source_finding_ids,
            source_request_id,
        };
    }
    let outcome = try_append_reminder(
        body_bytes,
        source,
        reminder_text,
        source_finding_ids,
        source_request_id,
        request_id,
    );
    if matches!(outcome, RemindersOutcome::Appended { .. }) {
        push_deps.gate.record_delivery(session_id, &probe);
    }
    outcome
}

fn try_append_reminder(
    body_bytes: &[u8],
    source: ReminderSource,
    reminder_text: String,
    source_finding_ids: Vec<i64>,
    source_request_id: u64,
    request_id: RequestId,
) -> RemindersOutcome {
    let mut post = bytes::Bytes::from(body_bytes.to_vec());
    let mut stored: Option<bytes::Bytes> = None;
    let mut applied = false;
    let reminder = SystemReminder {
        source,
        payload: reminder_text.clone(),
    };
    if !inject::inject_system_reminder(
        &reminder,
        &mut inject::InjectBookkeeping {
            post_filter_body: &mut post,
            filtered_stored: &mut stored,
            filter_applied: &mut applied,
            request_id,
        },
    ) {
        return RemindersOutcome::Unchanged;
    }

    RemindersOutcome::Appended {
        body: post.to_vec(),
        reminder_text,
        source_finding_ids,
        source_request_id,
    }
}

pub async fn latest_findings(
    store: &tokio::sync::RwLock<RequestStore>,
    session_id: &str,
) -> Option<(u64, Vec<crate::store::models::Diagnostic>)> {
    let guard = store.read().await;
    if let Some((request_id, findings)) = guard.latest_findings(session_id) {
        return Some((request_id.as_u64(), findings));
    }
    let prev_id = guard.requests_by_session_id(session_id).pop()?;
    let entry = guard.get(&prev_id)?;
    if entry.diagnostics.is_empty() {
        return None;
    }
    Some((prev_id.as_u64(), entry.diagnostics.clone()))
}

pub async fn apply_reminders(
    body_bytes: &[u8],
    session_id: &str,
    store: &tokio::sync::RwLock<RequestStore>,
    cfg: &RemindersConfig,
    request_id: RequestId,
    push_deps: &crate::proxy::remote_control::inject::PushDeps,
) -> RemindersOutcome {
    if !cfg.enabled {
        return RemindersOutcome::Unchanged;
    }

    let Some((source_request_id, findings)) = latest_findings(store, session_id).await else {
        return RemindersOutcome::Unchanged;
    };

    let reminder_text = match formatter::build_reminder_text(&findings) {
        Some(text) => text,
        None => return RemindersOutcome::Unchanged,
    };

    let source_finding_ids: Vec<i64> = findings.iter().map(|d| d.match_range.0 as i64).collect();
    push_or_append_reminder(PushOrAppendRequest {
        push_deps,
        session_id,
        body_bytes,
        source: ReminderSource::Diagnostic,
        reminder_text,
        source_finding_ids,
        source_request_id,
        request_id,
    })
    .await
}
