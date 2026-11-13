use crate::store::persistence::runtime_repo::RuntimePersist as _;
use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use super::record::{ExchangeRecord, Finished};
use crate::runtime::{ModelCallObservation, ObservationSource, SessionObservation, UsageSnapshot};
use crate::store::persistence::PersistHandle;

pub async fn resolve_identity(
    persist: &PersistHandle,
    record: &mut ExchangeRecord,
) -> Result<(), String> {
    let thread = record
        .correlation
        .provider_thread_id
        .as_deref()
        .or(record.correlation.provider_session_id.as_deref());
    record.correlation.session_uuid = match (record.codex_home.as_deref(), thread) {
        (Some(home), Some(thread)) => {
            crate::runtime::codex_observe::resolve_bound_session(persist, home, thread)
                .await
                .map_err(|error| error.to_string())?
        }
        (None, Some(thread)) => persist
            .runtime()
            .resolve_unique_codex_session(thread)
            .await
            .map_err(|error| error.to_string())?,
        _ => None,
    };
    record.correlation.evidence["normalized_session"] = match record.correlation.session_uuid {
        Some(session) => {
            serde_json::json!({"source":if record.codex_home.is_some() { "verified_home_binding" } else { "unique_existing_thread_binding" },"session_id":session})
        }
        None => {
            serde_json::json!({"unknown":"no unambiguous existing thread binding; provider IDs are retained separately"})
        }
    };
    Ok(())
}

pub async fn record(
    persist: &PersistHandle,
    record: &ExchangeRecord,
    finished: &Finished,
) -> Result<(), String> {
    let Some(session_id) = record.correlation.session_uuid else {
        return Ok(());
    };
    if record.websocket_refused
        || !record
            .path
            .split('?')
            .next()
            .is_some_and(|path| path.ends_with("/responses"))
    {
        return Ok(());
    }
    persist
        .ensure_runtime_session(session_id)
        .await
        .map_err(|error| error.to_string())?;
    let summary = &finished.summary;
    let observation_id = Uuid::new_v4();
    let now = Utc::now();
    let observation = SessionObservation {
        session_id,
        observation_id,
        runtime: "codex".into(),
        source: ObservationSource {
            kind: "responses_wire".into(),
            version: None,
            fidelity: "normalized_metadata".into(),
            completeness: if summary.oversized_frames + summary.undecodable_frames == 0 {
                "usage_and_status"
            } else {
                "missing_frames"
            }
            .into(),
        },
        observed_at: now,
        sequence: 0,
        lifecycle: None,
        requested_model: record.model.clone(),
        observed_model: summary.model.clone(),
        usage: None,
        control: None,
        item: None,
        model_call: Some(ModelCallObservation {
            call_id: summary
                .response_id
                .clone()
                .unwrap_or_else(|| observation_id.to_string()),
            status: model_call_state(summary, record.response_status, finished.status)
                .map(str::to_owned),
            duration_ms: Some((now - record.created_at).num_milliseconds()),
            http_status: record
                .response_status
                .and_then(|status| status.try_into().ok()),
            usage: summary.usage.as_ref().map(usage),
        }),
    };
    use crate::store::persistence::runtime::ObservationApplyOutcome;
    match persist
        .runtime()
        .apply_session_observation(&observation)
        .await
        .map_err(|error| error.to_string())?
    {
        ObservationApplyOutcome::Applied | ObservationApplyOutcome::Replayed => Ok(()),
        ObservationApplyOutcome::BudgetRefused => {
            Err("normalized wire observation refused by storage budget".into())
        }
        ObservationApplyOutcome::Expired => {
            Err("normalized wire observation is outside retention".into())
        }
    }
}

fn model_call_state(
    summary: &mike_codex::responses::ResponsesSummary,
    http_status: Option<u16>,
    transport: super::record::ExchangeStatus,
) -> Option<&str> {
    let state = summary.model_call_state(http_status);
    if summary.terminal
        || state == Some("failed")
        || transport == super::record::ExchangeStatus::Complete
    {
        state
    } else {
        Some(transport.as_db_str())
    }
}

fn usage(value: &Value) -> UsageSnapshot {
    let token = |pointer| {
        value
            .pointer(pointer)
            .and_then(Value::as_i64)
            .filter(|count| *count >= 0)
    };
    UsageSnapshot {
        scope: "own".into(),
        basis: "call".into(),
        input_tokens: token("/input_tokens"),
        cached_input_tokens: token("/input_tokens_details/cached_tokens"),
        output_tokens: token("/output_tokens"),
        reasoning_output_tokens: token("/output_tokens_details/reasoning_tokens"),
        total_tokens: token("/total_tokens"),
        context_window: None,
    }
}

#[cfg(test)]
mod tests {
    use super::model_call_state;
    use crate::proxy::responses_capture::record::ExchangeStatus;
    use mike_codex::responses::ResponsesSummary;

    #[test]
    fn interrupted_transport_cannot_leave_a_started_call_running() {
        let mut summary = ResponsesSummary {
            state: Some("in_progress".into()),
            ..Default::default()
        };
        assert_eq!(
            model_call_state(&summary, Some(200), ExchangeStatus::Aborted),
            Some("aborted")
        );
        summary.state = Some("completed".into());
        summary.terminal = true;
        assert_eq!(
            model_call_state(&summary, Some(200), ExchangeStatus::Aborted),
            Some("completed")
        );
    }
}
