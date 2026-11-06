use bytes::Bytes;
use serde_json::json;

use super::*;
use crate::extract::request::TurnOrigin;
use crate::feature_toggles::ResolvedToggles;
use crate::filter::FilterConfig;
use crate::lesson_worker::LessonSnapshot;
use crate::store::models::RequestId;

fn bash_then_answers(answers: usize) -> anyhow::Result<Vec<u8>> {
    let output: String = (1..=300)
        .map(|i| format!("crates/mike/src/listing_{i}.rs: a line long enough to count\n"))
        .collect();
    let mut messages = vec![
        json!({"role": "assistant", "content": [
            {"type": "tool_use", "id": "b1", "name": "Bash", "input": {"command": "rg -l x"}}
        ]}),
        json!({"role": "user", "content": [{"type": "tool_result", "tool_use_id": "b1", "content": output}]}),
    ];
    for _ in 0..answers {
        messages.push(json!({"role": "assistant", "content": [{"type": "text", "text": "noted"}]}));
        messages.push(json!({"role": "user", "content": [{"type": "text", "text": "continue"}]}));
    }
    Ok(serde_json::to_vec(
        &json!({"model": "m", "messages": messages}),
    )?)
}

async fn run(toggles: &ResolvedToggles, state: &crate::config::AppState, raw: &[u8]) -> StageBody {
    let filter_cfg = FilterConfig::default();
    let lesson_snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::Primary;
    let ctx = StageContext {
        state,
        session_toggles: toggles,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &lesson_snapshot,
        session_id: "session-shell-output-stage-test",
        project_path: "",
        turn_origin: &turn_origin,
        request_id: RequestId::next(),
        is_tool_loop: false,
        subagent_persona: None,
        has_agent_id: false,
    };
    let mut body = StageBody::new(Bytes::from(raw.to_vec()), "orig".to_string());
    ShellOutputCompressionStage.apply(&ctx, &mut body).await;
    body
}

#[tokio::test]
async fn shadow_arm_records_the_cut_and_forwards_the_body_unchanged() -> anyhow::Result<()> {
    let state =
        crate::config::AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let toggles: ResolvedToggles = (*state.resolve_session_toggles("shell-shadow")).clone();
    assert!(!toggles.shell_output_compression, "the toggle ships off");
    let raw = bash_then_answers(3)?;
    let body = run(&toggles, &state, &raw).await;
    assert_eq!(body.body().as_ref(), raw.as_slice());
    assert!(!body.filter_applied());
    let manifest = body.into_parts().manifest.finish("orig".to_string());
    let stage = &manifest.stages[0];
    assert_eq!(stage.stage, STAGE);
    assert_eq!(stage.outcome, StageOutcome::Unchanged);
    assert_eq!(stage.reason_code, SHADOW_CANDIDATE);
    assert_eq!(stage.resolved_parameters["arm"], ARM_CONTROL);
    assert_eq!(stage.resolved_parameters["would_reduce"], 1);
    assert!(
        stage.resolved_parameters["saved_bytes"]
            .as_u64()
            .unwrap_or(0)
            > 10_000
    );
    assert_eq!(stage.targets[0].outcome, StageOutcome::Applied);
    assert!(stage.targets[0].recovery_ref.is_some());
    assert_eq!(manifest.verify_chain(), Ok(()));
    Ok(())
}

#[tokio::test]
async fn forward_arm_on_a_cold_cache_sends_the_shortened_body() -> anyhow::Result<()> {
    let state =
        crate::config::AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let mut toggles: ResolvedToggles = (*state.resolve_session_toggles("shell-forward")).clone();
    toggles.shell_output_compression_forward = true;
    let raw = bash_then_answers(3)?;
    crate::compress::cache_gate::seed_cold_lane(REDUCER, &raw);
    let body = run(&toggles, &state, &raw).await;
    assert!(body.body().len() < raw.len());
    let forwarded = String::from_utf8(body.body().to_vec())?;
    assert!(forwarded.contains(shell_output::REDUCTION_MARKER_TAG));
    let manifest = body
        .into_parts()
        .manifest
        .finish(sha256_hex(forwarded.as_bytes()));
    assert_eq!(manifest.stages[0].reason_code, FORWARDED);
    assert_eq!(manifest.stages[0].resolved_parameters["arm"], ARM_FORWARD);
    Ok(())
}

#[tokio::test]
async fn session_arm_uses_its_own_share_independent_of_file_reads() -> anyhow::Result<()> {
    let state =
        crate::config::AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let mut toggles: ResolvedToggles = (*state.resolve_session_toggles("shell-arm")).clone();
    toggles.file_read_compression_forward_share = 100;
    assert!((0..50).all(|i| session_arm(&toggles, &format!("s-{i}")) == ARM_CONTROL));
    toggles.shell_output_compression_forward_share = 50;
    let forwarded = (0..200)
        .filter(|i| session_arm(&toggles, &format!("s-{i}")) == ARM_FORWARD)
        .count();
    assert!((60..140).contains(&forwarded), "{forwarded}");
    Ok(())
}
