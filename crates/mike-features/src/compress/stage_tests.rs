use crate::feature_toggles::ResolvedToggles;
use bytes::Bytes;
use serde_json::json;

use super::*;
use crate::extract::request::TurnOrigin;
use crate::filter::FilterConfig;
use crate::lesson_cache::LessonSnapshot;
use crate::store::models::RequestId;

fn file_read(toggles: &ResolvedToggles) -> FileReadCompressionOverlay {
    toggles.features.get::<FileReadCompressionOverlay>()
}

fn set_file_read(
    toggles: &mut ResolvedToggles,
    change: impl FnOnce(&mut FileReadCompressionOverlay),
) {
    toggles
        .features
        .update::<FileReadCompressionOverlay>(change);
}

fn big_file_content(lines: usize) -> String {
    (1..=lines)
        .map(|i| format!("line {i} of the file with enough padding to matter\n"))
        .collect()
}

fn make_ctx<'a>(
    state: &'a crate::state::AppState,
    session_toggles: &'a crate::feature_toggles::ResolvedToggles,
    filter_cfg: &'a FilterConfig,
    lesson_snapshot: &'a LessonSnapshot,
    turn_origin: &'a TurnOrigin,
) -> StageContext<'a> {
    StageContext {
        state,
        session_toggles,
        filter_cfg,
        lesson_snapshot,
        session_id: "session-file-read-compression-stage-test",
        project_path: "",
        turn_origin,
        request_id: RequestId::next(),
        is_tool_loop: false,
        subagent_persona: None,
        has_agent_id: false,
    }
}

#[tokio::test]
async fn enabled_follows_the_file_read_compression_toggle() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("session-file-read-compression-stage-test");
    let mut session_toggles: ResolvedToggles = (*resolved).clone();
    let filter_cfg = FilterConfig::default();
    let lesson_snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::Primary;

    session_toggles.file_read_compression = true;
    let ctx_on = make_ctx(
        &state,
        &session_toggles,
        &filter_cfg,
        &lesson_snapshot,
        &turn_origin,
    );
    assert!(FileReadCompressionStage.enabled(&ctx_on));

    session_toggles.file_read_compression = false;
    let ctx_off = make_ctx(
        &state,
        &session_toggles,
        &filter_cfg,
        &lesson_snapshot,
        &turn_origin,
    );
    assert!(!FileReadCompressionStage.enabled(&ctx_off));
    Ok(())
}

#[tokio::test]
async fn apply_computes_a_reduction_but_never_mutates_the_forwarded_body() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let session_toggles = state.resolve_session_toggles("session-file-read-compression-stage-test");
    let filter_cfg = FilterConfig::default();
    let lesson_snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::Primary;
    let ctx = make_ctx(
        &state,
        &session_toggles,
        &filter_cfg,
        &lesson_snapshot,
        &turn_origin,
    );

    let content = big_file_content(200);
    let raw = serde_json::to_vec(&json!({
        "model": "m",
        "system": [{"type": "text", "text": "SENTINEL-PREFIX-UNCHANGED"}],
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/big.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": content}
            ]},
            {"role": "assistant", "content": [{"type": "text", "text": "noted"}]},
            {"role": "user", "content": [{"type": "text", "text": "continue"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "noted"}]},
            {"role": "user", "content": [{"type": "text", "text": "continue"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "noted"}]},
            {"role": "user", "content": [{"type": "text", "text": "continue"}]}
        ]
    }))?;
    let sanity_check =
        file_reads::reduce_file_reads(&raw, &super::session_policy(&file_read(&session_toggles)));
    assert_eq!(
        sanity_check.stage.outcome,
        StageOutcome::Applied,
        "fixture must be a genuine reduction candidate for this test to prove anything"
    );

    let mut body = StageBody::new(Bytes::from(raw.clone()), "orig".to_string());
    FileReadCompressionStage.apply(&ctx, &mut body).await;

    assert!(
        !body.filter_applied(),
        "shadow mode must never set filter_applied"
    );
    assert!(body.filtered_stored().is_none());
    assert_eq!(
        body.body().as_ref(),
        raw.as_slice(),
        "shadow mode must forward the body byte-identical even though a reduction existed"
    );
    let parts = body.into_parts();
    let manifest = parts.manifest.finish("orig".to_string());
    assert_eq!(
        manifest.stages.len(),
        1,
        "shadow mode records exactly one entry for the computed reduction"
    );
    assert_eq!(
        manifest.stages[0].outcome,
        StageOutcome::Unchanged,
        "recording an Applied entry whose bytes were never forwarded would break the \
             manifest's hash-chain invariant"
    );
    assert_eq!(manifest.stages[0].reason_code, "file_read_shadow_candidate");
    assert_eq!(
        manifest.stages[0].input_sha256,
        manifest.stages[0].output_sha256
    );
    assert_eq!(manifest.verify_chain(), Ok(()));
    let stage = &manifest.stages[0];
    assert_eq!(stage.targets.len(), 1, "the would-be reduction is recorded");
    assert_eq!(stage.targets[0].outcome, StageOutcome::Applied);
    assert_eq!(stage.targets[0].reason_code, "file_read_reduced");
    let target_detail = stage.targets[0].detail.clone().unwrap_or_default();
    assert!(target_detail.contains("/tmp/big.rs"), "{target_detail}");
    assert!(target_detail.contains("kept file lines"), "{target_detail}");
    assert_eq!(stage.resolved_parameters["would_reduce"], 1);
    let stage_detail = stage.detail.clone().unwrap_or_default();
    assert!(
        stage_detail.contains("forwarded unchanged"),
        "{stage_detail}"
    );
    Ok(())
}

fn read_then_answers(answers: usize) -> anyhow::Result<Vec<u8>> {
    let mut messages = vec![
        json!({"role": "assistant", "content": [
            {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/big.md"}}
        ]}),
        json!({"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "t1", "content": big_file_content(200)}
        ]}),
    ];
    for _ in 0..answers {
        messages.push(json!({"role": "assistant", "content": [{"type": "text", "text": "noted"}]}));
        messages.push(json!({"role": "user", "content": [{"type": "text", "text": "continue"}]}));
    }
    Ok(serde_json::to_vec(
        &json!({"model": "m", "messages": messages}),
    )?)
}

#[tokio::test]
async fn forward_mode_sends_the_shortened_body_and_records_an_applied_stage() -> anyhow::Result<()>
{
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("session-file-read-forward-test");
    let mut session_toggles: ResolvedToggles = (*resolved).clone();
    set_file_read(&mut session_toggles, |o| o.mode = "forward".to_owned());
    let filter_cfg = FilterConfig::default();
    let lesson_snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::Primary;
    let ctx = make_ctx(
        &state,
        &session_toggles,
        &filter_cfg,
        &lesson_snapshot,
        &turn_origin,
    );
    let raw = read_then_answers(3)?;
    crate::compress::cache_gate::seed_cold_lane(super::GATE_REDUCER, &raw);
    let mut body = StageBody::new(Bytes::from(raw.clone()), "orig".to_string());
    FileReadCompressionStage.apply(&ctx, &mut body).await;

    assert!(
        body.body().len() < raw.len(),
        "forward mode must shorten the body"
    );
    let forwarded = String::from_utf8(body.body().to_vec())?;
    assert!(forwarded.contains("[[mike:file_read_reduced]]"));
    let manifest = body
        .into_parts()
        .manifest
        .finish(crate::wire::manifest::sha256_hex(forwarded.as_bytes()));
    let stage = &manifest.stages[0];
    assert_eq!(stage.outcome, StageOutcome::Applied);
    assert_eq!(stage.reason_code, "file_read_forwarded");
    assert_eq!(stage.targets[0].reason_code, "file_read_reduced");
    assert_eq!(manifest.verify_chain(), Ok(()));
    Ok(())
}

#[tokio::test]
async fn the_forward_share_splits_sessions_into_stable_arms() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("session-file-read-arm-test");
    let mut toggles: ResolvedToggles = (*resolved).clone();
    let ids: Vec<String> = (0..400).map(|i| format!("session-{i}")).collect();

    set_file_read(&mut toggles, |o| o.forward_share = 0);
    assert!(
        ids.iter()
            .all(|id| super::session_arm(&toggles, id) == super::ARM_CONTROL)
    );

    set_file_read(&mut toggles, |o| o.forward_share = 100);
    assert!(
        ids.iter()
            .all(|id| super::session_arm(&toggles, id) == super::ARM_FORWARD)
    );

    set_file_read(&mut toggles, |o| o.forward_share = 50);
    let forward = ids
        .iter()
        .filter(|id| super::session_arm(&toggles, id) == super::ARM_FORWARD)
        .count();
    assert!(
        (140..=260).contains(&forward),
        "{forward} of 400 forwarded at 50%"
    );
    assert_eq!(
        super::session_arm(&toggles, "session-7"),
        super::session_arm(&toggles, "session-7")
    );
    Ok(())
}

#[tokio::test]
async fn forward_mode_forwards_every_session_and_running_sessions_keep_their_arm()
-> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let mut toggles: ResolvedToggles = (*state.resolve_session_toggles("arm-a")).clone();
    set_file_read(&mut toggles, |o| {
        o.mode = "forward".to_owned();
        o.forward_share = 0;
    });
    assert!(
        (0..100)
            .map(|i| format!("session-{i}"))
            .all(|id| super::session_arm(&toggles, &id) == super::ARM_FORWARD)
    );

    let running = state.resolve_session_toggles("arm-running");
    assert_eq!(
        super::session_arm(&running, "arm-running"),
        super::ARM_CONTROL
    );
    state.settings_overlay.store(std::sync::Arc::new({
        let mut overlay = crate::settings::SettingsOverlay::default();
        overlay
            .features
            .update::<FileReadCompressionOverlay>(|o| o.mode = "forward".to_owned());
        overlay
    }));
    let still = state.resolve_session_toggles("arm-running");
    assert_eq!(
        super::session_arm(&still, "arm-running"),
        super::ARM_CONTROL
    );
    let new = state.resolve_session_toggles("arm-new");
    assert_eq!(super::session_arm(&new, "arm-new"), super::ARM_FORWARD);
    Ok(())
}

#[test]
fn effective_forwarding_names_shadow_sampled_and_forced_states() {
    let pure = super::effective_forwarding("shadow", 0);
    assert_eq!(pure["state"], "pure_shadow");
    let sampled = super::effective_forwarding("shadow", 20);
    assert_eq!(sampled["state"], "sampled_forwarding");
    assert!(
        sampled["summary"]
            .as_str()
            .is_some_and(|s| s.starts_with("20%"))
    );
    assert_eq!(
        super::effective_forwarding("forward", 0)["state"],
        "forced_forwarding"
    );
    assert_eq!(
        super::effective_forwarding("forward", 20)["state"],
        "forced_forwarding"
    );
}

#[tokio::test]
async fn the_default_grace_window_keeps_a_read_answered_twice_whole() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let session_toggles = state.resolve_session_toggles("session-file-read-grace-test");
    let settings = file_read(&session_toggles);
    assert_eq!(settings.grace_turns, 3);
    assert!(!settings.forward(), "shadow is the default");
    let policy = super::session_policy(&settings);

    let two = file_reads::reduce_file_reads(&read_then_answers(2)?, &policy);
    assert_eq!(two.stage.outcome, StageOutcome::Unchanged);
    assert_eq!(two.stage.targets[0].reason_code, "grace_window");

    let three = file_reads::reduce_file_reads(&read_then_answers(3)?, &policy);
    assert_eq!(three.stage.outcome, StageOutcome::Applied);
    Ok(())
}

#[tokio::test]
async fn apply_records_a_skipped_read_as_a_count_and_not_as_a_target() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let session_toggles = state.resolve_session_toggles("session-file-read-compression-stage-test");
    let filter_cfg = FilterConfig::default();
    let lesson_snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::Primary;
    let ctx = make_ctx(
        &state,
        &session_toggles,
        &filter_cfg,
        &lesson_snapshot,
        &turn_origin,
    );
    let raw = serde_json::to_vec(&json!({
        "model": "m",
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/small.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fn main() {}\n"}
            ]},
            {"role": "assistant", "content": [{"type": "text", "text": "noted"}]},
            {"role": "user", "content": [{"type": "text", "text": "continue"}]}
        ]
    }))?;
    let mut body = StageBody::new(Bytes::from(raw), "orig".to_string());
    FileReadCompressionStage.apply(&ctx, &mut body).await;
    let manifest = body.into_parts().manifest.finish("orig".to_string());
    let stage = &manifest.stages[0];
    assert_eq!(stage.outcome, StageOutcome::Unchanged);
    assert_eq!(stage.reason_code, "no_candidates");
    assert!(
        stage.targets.is_empty(),
        "a skipped read is a count, not a target"
    );
    assert_eq!(stage.resolved_parameters["skipped"]["small_read"], 1);
    assert_eq!(stage.resolved_parameters["would_reduce"], 0);
    assert_eq!(
        stage.resolved_parameters["policy"]["min_result_bytes"],
        1500
    );
    assert!(stage.detail.is_none());
    Ok(())
}

#[tokio::test]
async fn apply_leaves_body_untouched_when_no_reduction_candidate() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let session_toggles = state.resolve_session_toggles("session-file-read-compression-stage-test");
    let filter_cfg = FilterConfig::default();
    let lesson_snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::Primary;
    let ctx = make_ctx(
        &state,
        &session_toggles,
        &filter_cfg,
        &lesson_snapshot,
        &turn_origin,
    );

    let raw = serde_json::to_vec(&json!({
        "model": "m",
        "messages": [{"role": "user", "content": [{"type": "text", "text": "hi"}]}]
    }))?;
    let mut body = StageBody::new(Bytes::from(raw.clone()), "orig".to_string());
    FileReadCompressionStage.apply(&ctx, &mut body).await;

    assert!(!body.filter_applied());
    assert_eq!(body.body().as_ref(), raw.as_slice());
    let manifest = body.into_parts().manifest.finish("orig".to_string());
    assert_eq!(manifest.stages.len(), 1);
    assert_eq!(manifest.stages[0].stage, "file_read_compression");
    assert_eq!(manifest.stages[0].outcome, StageOutcome::Unchanged);
    assert_ne!(manifest.stages[0].reason_code, "file_read_shadow_candidate");
    Ok(())
}

#[tokio::test]
async fn default_settings_policy_matches_pre_wiring_hardcoded_defaults() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let session_toggles =
        state.resolve_session_toggles("session-file-read-compression-default-parity-test");

    let content = big_file_content(200);
    let raw = serde_json::to_vec(&json!({
        "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/tmp/big.rs"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": content}
            ]}
        ]
    }))?;

    let hardcoded_producers = vec!["Read".to_string()];
    let hardcoded_protected: Vec<String> = Vec::new();
    let hardcoded_policy = file_reads::FileReadPolicy::new(
        file_reads::DEFAULT_MIN_RESULT_BYTES,
        (
            file_reads::DEFAULT_HEAD_LINES,
            file_reads::DEFAULT_TAIL_LINES,
        ),
        file_reads::DEFAULT_GRACE_TURNS,
        &hardcoded_producers,
        &hardcoded_protected,
    );
    let from_hardcoded = file_reads::reduce_file_reads(&raw, &hardcoded_policy);

    let settings = file_read(&session_toggles);
    let settings_policy = super::session_policy(&settings);
    let from_settings = file_reads::reduce_file_reads(&raw, &settings_policy);

    assert_eq!(from_hardcoded.candidate, from_settings.candidate);
    assert_eq!(from_hardcoded.stage.outcome, from_settings.stage.outcome);
    assert_eq!(
        from_hardcoded.stage.targets.len(),
        from_settings.stage.targets.len()
    );
    Ok(())
}
