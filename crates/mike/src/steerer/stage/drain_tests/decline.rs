use bytes::Bytes;

use crate::config::{AppState, ProxyConfig};
use crate::extract::request::TurnOrigin;
use crate::reminders::inject::InjectBookkeeping;
use crate::steerer::stage::drain;
use crate::store::models::{ReminderSource, RequestId, SystemReminder};
use crate::wire::manifest::{ManifestBuilder, StageOutcome, TransformationManifest};

const SESSION: &str = "session-steerer-drain-decline";

fn user_turn() -> Bytes {
    Bytes::from_static(
        br#"{"model":"m","messages":[{"role":"user","content":[{"type":"text","text":"q"}]}]}"#,
    )
}

async fn drain_once(state: &AppState) -> TransformationManifest {
    let mut body = user_turn();
    let mut filtered_stored = None;
    let mut filter_applied = false;
    let mut manifest = ManifestBuilder::new("test".to_string());
    let mut bk = InjectBookkeeping {
        post_filter_body: &mut body,
        filtered_stored: &mut filtered_stored,
        filter_applied: &mut filter_applied,
        request_id: RequestId::next(),
    };
    let injected = drain(
        state,
        SESSION,
        &TurnOrigin::Primary,
        "",
        &mut bk,
        &mut manifest,
    )
    .await;
    assert!(injected.is_empty());
    manifest.finish("test".to_string())
}

#[tokio::test]
async fn an_empty_queue_records_steerer_queue_empty() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    let manifest = drain_once(&state).await;

    assert_eq!(manifest.stages.len(), 1);
    let entry = &manifest.stages[0];
    assert_eq!(entry.stage, "steerer");
    assert_eq!(entry.outcome, StageOutcome::Unchanged);
    assert_eq!(entry.reason_code, "steerer_queue_empty");
    assert_eq!(entry.resolved_parameters["pending_count"], 0);
    assert_eq!(manifest.verify_chain(), Ok(()));
    Ok(())
}

#[tokio::test]
async fn a_reminder_the_gate_refuses_is_recorded_with_the_gate_reason() -> anyhow::Result<()> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    let reminder = SystemReminder {
        source: ReminderSource::Recall,
        payload: "<system-reminder>already shown</system-reminder>".to_string(),
    };
    state.injection_gate.record_delivery(SESSION, &reminder);
    state
        .store
        .write()
        .await
        .enqueue_reminder(SESSION, reminder);

    let manifest = drain_once(&state).await;

    assert_eq!(manifest.stages.len(), 1);
    let entry = &manifest.stages[0];
    assert_eq!(entry.outcome, StageOutcome::Unchanged);
    assert_eq!(entry.reason_code, "steerer_gate_suppressed");
    assert_eq!(entry.resolved_parameters["pending_count"], 1);
    assert_eq!(
        entry.resolved_parameters["gate_suppressed"],
        serde_json::json!(["duplicate_content"])
    );
    assert_eq!(manifest.verify_chain(), Ok(()));
    Ok(())
}
