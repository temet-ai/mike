use bytes::Bytes;

use super::stage::SelfIdentityStage;
use super::stage_test_support::{StageCtxFixture, make_ctx, user_turn_body};
use crate::compress::manifest::StageOutcome;
use crate::extract::request::TurnOrigin;
use crate::feature_toggles::ResolvedToggles;
use crate::filter::FilterConfig;
use crate::lesson_worker::LessonSnapshot;
use crate::pipeline::{RequestStage, StageBody};

#[tokio::test]
async fn self_identity_enabled_requires_toggle_primary_turn_and_no_tool_loop() -> anyhow::Result<()>
{
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::config::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("self-identity-stage-enabled-test");
    let mut toggles_on: ResolvedToggles = (*resolved).clone();
    toggles_on.self_identity_reminder = true;
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let primary = TurnOrigin::Primary;

    let ctx_on = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &toggles_on,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &primary,
        session_id: "s",
        project_path: "proj",
        is_tool_loop: false,
        subagent_persona: None,
    });
    assert!(SelfIdentityStage.enabled(&ctx_on));

    let ctx_toggle_off = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &primary,
        session_id: "s",
        project_path: "proj",
        is_tool_loop: false,
        subagent_persona: None,
    });
    assert!(!SelfIdentityStage.enabled(&ctx_toggle_off));

    let ctx_tool_loop = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &toggles_on,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &primary,
        session_id: "s",
        project_path: "proj",
        is_tool_loop: true,
        subagent_persona: None,
    });
    assert!(!SelfIdentityStage.enabled(&ctx_tool_loop));

    let subagent = TurnOrigin::Subagent;
    let ctx_wrong_origin = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &toggles_on,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &subagent,
        session_id: "s",
        project_path: "proj",
        is_tool_loop: false,
        subagent_persona: None,
    });
    assert!(!SelfIdentityStage.enabled(&ctx_wrong_origin));
    Ok(())
}

#[tokio::test]
async fn self_identity_apply_injects_on_first_request_and_skips_while_marker_still_present()
-> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::config::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("self-identity-stage-apply-test");
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::PrimaryFirst;
    let ctx = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &turn_origin,
        session_id: "self-identity-stage-apply-test",
        project_path: "my-project",
        is_tool_loop: false,
        subagent_persona: None,
    });

    let raw = user_turn_body()?;
    let mut body = StageBody::new(Bytes::from(raw.clone()), "orig".to_string());
    SelfIdentityStage.apply(&ctx, &mut body).await;

    assert!(body.filter_applied());
    assert!(
        String::from_utf8_lossy(body.body()).contains("self-identity-stage-apply-test"),
        "reminder text must name this session's own id"
    );
    let injected_bytes = body.body().to_vec();
    let parts = body.into_parts();
    let manifest = parts.manifest.finish("orig".to_string());
    assert_eq!(manifest.stages.len(), 1);
    assert_eq!(manifest.stages[0].stage, "self_identity");

    let mut body2 = StageBody::new(Bytes::from(injected_bytes.clone()), "orig".to_string());
    SelfIdentityStage.apply(&ctx, &mut body2).await;
    assert!(
        !body2.filter_applied(),
        "the identity block is still present in this request's own history — do not repeat it"
    );
    assert_eq!(body2.body().as_ref(), injected_bytes.as_slice());
    let suppressed = body2.into_parts().manifest.finish("orig".to_string());
    assert_eq!(suppressed.stages.len(), 1);
    assert_eq!(suppressed.stages[0].stage, "self_identity");
    assert_eq!(suppressed.stages[0].outcome, StageOutcome::Unchanged);
    assert_eq!(
        suppressed.stages[0].reason_code, "self_identity_suppressed_already_in_request",
        "a suppressed duplicate must be distinguishable in the manifest from a stage with nothing to say"
    );
    Ok(())
}

#[tokio::test]
async fn self_identity_apply_reinjects_once_the_marker_drops_out_of_the_request()
-> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::config::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("self-identity-stage-reassert-test");
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::PrimaryFirst;
    let ctx = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &turn_origin,
        session_id: "self-identity-stage-reassert-test",
        project_path: "my-project",
        is_tool_loop: false,
        subagent_persona: None,
    });

    let raw = user_turn_body()?;
    let mut first = StageBody::new(Bytes::from(raw.clone()), "orig".to_string());
    SelfIdentityStage.apply(&ctx, &mut first).await;
    assert!(first.filter_applied(), "first turn must inject the block");

    let mut compacted = StageBody::new(Bytes::from(raw.clone()), "orig".to_string());
    SelfIdentityStage.apply(&ctx, &mut compacted).await;
    assert!(
        compacted.filter_applied(),
        "a fresh request that no longer carries the identity marker (e.g. after \
         compaction dropped it) must have it reasserted, even though this session \
         already received it once"
    );
    assert!(
        String::from_utf8_lossy(compacted.body()).contains("self-identity-stage-reassert-test"),
        "the reasserted reminder must still name this session's own id"
    );
    Ok(())
}

#[tokio::test]
async fn self_identity_apply_leaves_body_untouched_when_not_a_user_turn() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::config::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("self-identity-stage-non-user-turn-test");
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::Primary;
    let ctx = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &turn_origin,
        session_id: "self-identity-stage-non-user-turn-test",
        project_path: "my-project",
        is_tool_loop: false,
        subagent_persona: None,
    });

    let raw = b"not json".to_vec();
    let mut body = StageBody::new(Bytes::from(raw.clone()), "orig".to_string());
    SelfIdentityStage.apply(&ctx, &mut body).await;

    assert!(!body.filter_applied());
    assert_eq!(body.body().as_ref(), raw.as_slice());
    let nothing = body.into_parts().manifest.finish("orig".to_string());
    assert_eq!(nothing.stages.len(), 1);
    assert_eq!(
        nothing.stages[0].reason_code, "self_identity_nothing_to_inject",
        "a stage that had nothing to inject carries its own reason code"
    );
    Ok(())
}

#[tokio::test]
async fn self_identity_enabled_only_on_session_own_lane_without_agent_id() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::config::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("self_identity-own-lane-test");
    let mut toggles_on: ResolvedToggles = (*resolved).clone();
    toggles_on.self_identity_reminder = true;
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let primary = TurnOrigin::Primary;
    let subagent = TurnOrigin::Subagent;
    let fixture = |origin| StageCtxFixture {
        state: &state,
        session_toggles: &toggles_on,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: origin,
        session_id: "s",
        project_path: "proj",
        is_tool_loop: false,
        subagent_persona: None,
    };

    let own_lane = make_ctx(fixture(&primary));
    assert!(SelfIdentityStage.enabled(&own_lane));

    let mut agent_on_primary = make_ctx(fixture(&primary));
    agent_on_primary.has_agent_id = true;
    assert!(!SelfIdentityStage.enabled(&agent_on_primary));

    let subagent_lane = make_ctx(fixture(&subagent));
    assert!(!SelfIdentityStage.enabled(&subagent_lane));
    Ok(())
}

#[tokio::test]
async fn self_identity_apply_leaves_a_template_session_body_unchanged() -> anyhow::Result<()> {
    let state =
        crate::config::AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let session_id = "self-identity-template-session-test";
    super::seat_role::remember_for_test(session_id, super::seat_role::SeatRole::Template);
    let resolved = state.resolve_session_toggles(session_id);
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::PrimaryFirst;
    let ctx = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &turn_origin,
        session_id,
        project_path: "my-project",
        is_tool_loop: false,
        subagent_persona: None,
    });

    let raw = user_turn_body()?;
    let mut body = StageBody::new(Bytes::from(raw.clone()), "orig".to_string());
    SelfIdentityStage.apply(&ctx, &mut body).await;

    assert!(!body.filter_applied());
    assert_eq!(body.body().as_ref(), raw.as_slice());
    let manifest = body.into_parts().manifest.finish("orig".to_string());
    assert_eq!(manifest.stages.len(), 1);
    assert_eq!(manifest.stages[0].outcome, StageOutcome::Unchanged);
    assert_eq!(
        manifest.stages[0].reason_code,
        "self_identity_template_session"
    );
    Ok(())
}
