use bytes::Bytes;

use super::stage::RemindersStage;
use super::stage_test_support::{
    StageCtxFixture, make_ctx, mk_finding, sample_request, user_turn_body,
};
use crate::extract::request::TurnOrigin;
use crate::feature_toggles::ResolvedToggles;
use crate::filter::FilterConfig;
use crate::lesson_worker::LessonSnapshot;
use crate::pipeline::{RequestStage, StageBody};
use crate::store::models::RequestId;

#[tokio::test]
async fn reminders_enabled_follows_the_reminders_toggle() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("reminders-stage-enabled-test");
    let mut toggles_on: ResolvedToggles = (*resolved).clone();
    toggles_on.reminders = true;
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::Primary;

    let ctx_on = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &toggles_on,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &turn_origin,
        session_id: "s",
        project_path: "",
        is_tool_loop: false,
        subagent_persona: None,
    });
    assert!(RemindersStage.enabled(&ctx_on));

    let ctx_off = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &turn_origin,
        session_id: "s",
        project_path: "",
        is_tool_loop: false,
        subagent_persona: None,
    });
    assert!(!RemindersStage.enabled(&ctx_off));
    Ok(())
}

#[tokio::test]
async fn reminders_apply_appends_reminder_from_prior_findings() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("reminders-stage-apply-test");
    let mut toggles_on: ResolvedToggles = (*resolved).clone();
    toggles_on.reminders = true;
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::Primary;
    let session_id = "reminders-stage-apply-test";
    {
        let mut guard = state.store.write().await;
        let mut req = sample_request(RequestId::from_raw(42), session_id);
        req.diagnostics = vec![mk_finding(
            "comprehensive solution",
            "no MVP without ask",
            0,
        )];
        guard.insert(req);
    }
    let ctx = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &toggles_on,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &turn_origin,
        session_id,
        project_path: "",
        is_tool_loop: false,
        subagent_persona: None,
    });

    let raw = user_turn_body()?;
    let mut body = StageBody::new(Bytes::from(raw), "orig".to_string());
    RemindersStage.apply(&ctx, &mut body).await;

    assert!(body.filter_applied());
    assert!(String::from_utf8_lossy(body.body()).contains("comprehensive solution"));
    let parts = body.into_parts();
    let manifest = parts.manifest.finish("orig".to_string());
    assert_eq!(manifest.stages.len(), 1);
    assert_eq!(manifest.stages[0].stage, "reminders");
    Ok(())
}

#[tokio::test]
async fn reminders_apply_leaves_body_untouched_when_no_prior_findings() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("reminders-stage-no-prior-test");
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::Primary;
    let ctx = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &turn_origin,
        session_id: "reminders-stage-no-prior-test",
        project_path: "",
        is_tool_loop: false,
        subagent_persona: None,
    });

    let raw = user_turn_body()?;
    let mut body = StageBody::new(Bytes::from(raw.clone()), "orig".to_string());
    RemindersStage.apply(&ctx, &mut body).await;

    assert!(!body.filter_applied());
    assert!(body.filtered_stored().is_none());
    assert_eq!(body.body().as_ref(), raw.as_slice());
    Ok(())
}

#[tokio::test]
async fn diagnostic_reminders_enabled_only_on_session_own_lane_without_agent_id()
-> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("diagnostic_reminders-own-lane-test");
    let mut toggles_on: ResolvedToggles = (*resolved).clone();
    toggles_on.reminders = true;
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
    assert!(RemindersStage.enabled(&own_lane));

    let mut agent_on_primary = make_ctx(fixture(&primary));
    agent_on_primary.has_agent_id = true;
    assert!(!RemindersStage.enabled(&agent_on_primary));

    let subagent_lane = make_ctx(fixture(&subagent));
    assert!(!RemindersStage.enabled(&subagent_lane));
    Ok(())
}
