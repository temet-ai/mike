use bytes::Bytes;
use serde_json::json;

use super::stage::ReadDedupStage;
use crate::extract::request::TurnOrigin;
use crate::feature_toggles::ResolvedToggles;
use crate::filter::FilterConfig;
use crate::lesson_worker::LessonSnapshot;
use crate::pipeline::{RequestStage, StageBody, StageContext};
use crate::store::models::RequestId;
use crate::wire::manifest::{StageEntry, StageOutcome};

fn reads(tool_use_ids: &[&str]) -> anyhow::Result<Vec<u8>> {
    let mut messages = Vec::new();
    for id in tool_use_ids {
        messages.push(json!({"role": "assistant", "content": [
            {"type": "tool_use", "id": id, "name": "Read", "input": {"file_path": "/tmp/a.rs"}}
        ]}));
        messages.push(json!({"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": id, "content": "fn main() {}"}
        ]}));
    }
    Ok(serde_json::to_vec(&json!({ "messages": messages }))?)
}

async fn apply_once(
    state: &crate::config::AppState,
    session_toggles: &ResolvedToggles,
    session_id: &str,
    raw: Vec<u8>,
) -> StageBody {
    let filter_cfg = FilterConfig::default();
    let lesson_snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::Primary;
    let ctx = StageContext {
        state,
        session_toggles,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &lesson_snapshot,
        session_id,
        project_path: "",
        turn_origin: &turn_origin,
        request_id: RequestId::next(),
        is_tool_loop: false,
        subagent_persona: None,
        has_agent_id: false,
    };
    let mut body = StageBody::new(Bytes::from(raw), "orig".to_string());
    ReadDedupStage.apply(&ctx, &mut body).await;
    body
}

fn only_entry(body: StageBody) -> anyhow::Result<StageEntry> {
    let manifest = body.into_parts().manifest.finish("orig".to_string());
    anyhow::ensure!(
        manifest.stages.len() == 1,
        "expected exactly one manifest entry, got {:?}",
        manifest.stages
    );
    Ok(manifest.stages[0].clone())
}

#[tokio::test]
async fn a_body_that_is_not_json_records_a_decline() -> anyhow::Result<()> {
    let state =
        crate::config::AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let toggles = state.resolve_session_toggles("session-dedup-decline-not-json");
    let body = apply_once(
        &state,
        &toggles,
        "session-dedup-decline-not-json",
        b"not json".to_vec(),
    )
    .await;
    let entry = only_entry(body)?;
    assert_eq!(entry.reason_code, "read_dedup_body_not_json");
    assert_eq!(entry.outcome, StageOutcome::Unchanged);
    Ok(())
}

#[tokio::test]
async fn a_disarmed_duplicate_read_is_recorded_with_its_hit_count_and_savings() -> anyhow::Result<()>
{
    let state =
        crate::config::AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let session = "session-dedup-decline-disarmed";
    let mut toggles: ResolvedToggles = (*state.resolve_session_toggles(session)).clone();
    toggles.compression_dedup_enabled = false;

    apply_once(&state, &toggles, session, reads(&["t1"])?).await;
    let second = apply_once(&state, &toggles, session, reads(&["t1", "t2"])?).await;
    assert!(
        !second.filter_applied(),
        "a disarmed stage must forward the body untouched"
    );

    let entry = only_entry(second)?;
    assert_eq!(entry.outcome, StageOutcome::Unchanged);
    assert_eq!(entry.reason_code, "read_dedup_observed_disarmed");
    assert_eq!(entry.resolved_parameters["hit_count"], 1);
    assert_eq!(entry.resolved_parameters["armed"], false);
    assert!(
        entry.resolved_parameters["would_save_bytes"]
            .as_u64()
            .is_some_and(|bytes| bytes > 0),
        "the manifest must carry the bytes a rewrite would have saved: {:?}",
        entry.resolved_parameters
    );
    Ok(())
}

#[tokio::test]
async fn an_armed_rewrite_records_the_same_hit_parameters() -> anyhow::Result<()> {
    let state =
        crate::config::AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let session = "session-dedup-decline-armed";
    let mut toggles: ResolvedToggles = (*state.resolve_session_toggles(session)).clone();
    toggles.compression_dedup_enabled = true;

    apply_once(&state, &toggles, session, reads(&["t1"])?).await;
    let second = apply_once(&state, &toggles, session, reads(&["t1", "t2"])?).await;
    assert!(second.filter_applied());

    let entry = only_entry(second)?;
    assert_eq!(entry.outcome, StageOutcome::Applied);
    assert_eq!(entry.reason_code, "duplicate_read");
    assert_eq!(entry.resolved_parameters["hit_count"], 1);
    assert_eq!(entry.resolved_parameters["armed"], true);
    assert!(
        entry.resolved_parameters["would_save_bytes"]
            .as_u64()
            .is_some_and(|bytes| bytes > 0)
    );
    Ok(())
}
