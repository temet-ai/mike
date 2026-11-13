use bytes::Bytes;

use super::stage::AgentLessonsStage;
use super::stage_test_support::{StageCtxFixture, make_ctx, user_turn_body};
use super::test_support::code_reviewer_snapshot;
use crate::extract::request::TurnOrigin;
use crate::feature_toggles::ResolvedToggles;
use crate::filter::FilterConfig;
use crate::pipeline::{RequestStage, StageBody};

#[tokio::test]
async fn agent_lessons_enabled_requires_toggle_and_subagent_turn() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("agent-lessons-stage-enabled-test");
    let mut toggles_on: ResolvedToggles = (*resolved).clone();
    toggles_on.agent_lessons = true;
    let filter_cfg = FilterConfig::default();
    let snapshot = code_reviewer_snapshot();
    let subagent = TurnOrigin::Subagent;

    let ctx_on = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &toggles_on,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &subagent,
        session_id: "s",
        project_path: "",
        is_tool_loop: false,
        subagent_persona: Some("code-reviewer"),
    });
    assert!(AgentLessonsStage.enabled(&ctx_on));

    let ctx_toggle_off = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &subagent,
        session_id: "s",
        project_path: "",
        is_tool_loop: false,
        subagent_persona: Some("code-reviewer"),
    });
    assert!(!AgentLessonsStage.enabled(&ctx_toggle_off));

    let primary = TurnOrigin::Primary;
    let ctx_wrong_origin = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &toggles_on,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &primary,
        session_id: "s",
        project_path: "",
        is_tool_loop: false,
        subagent_persona: Some("code-reviewer"),
    });
    assert!(!AgentLessonsStage.enabled(&ctx_wrong_origin));
    Ok(())
}

#[tokio::test]
async fn agent_lessons_apply_injects_and_records_manifest() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("agent-lessons-stage-apply-test");
    let mut toggles_on: ResolvedToggles = (*resolved).clone();
    toggles_on.agent_lessons = true;
    let filter_cfg = FilterConfig::default();
    let snapshot = code_reviewer_snapshot();
    let turn_origin = TurnOrigin::Subagent;
    let ctx = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &toggles_on,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &turn_origin,
        session_id: "agent-lessons-stage-apply-test",
        project_path: "",
        is_tool_loop: false,
        subagent_persona: Some("code-reviewer"),
    });

    let raw = user_turn_body()?;
    let mut body = StageBody::new(Bytes::from(raw), "orig".to_string());
    AgentLessonsStage.apply(&ctx, &mut body).await;

    assert!(body.filter_applied());
    assert!(String::from_utf8_lossy(body.body()).contains("never approve without running tests"));
    let parts = body.into_parts();
    let manifest = parts.manifest.finish("orig".to_string());
    assert_eq!(manifest.stages.len(), 1);
    assert_eq!(manifest.stages[0].stage, "agent_lessons");
    Ok(())
}

#[tokio::test]
async fn agent_lessons_apply_leaves_body_untouched_when_persona_unresolved() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("agent-lessons-stage-no-persona-test");
    let filter_cfg = FilterConfig::default();
    let snapshot = code_reviewer_snapshot();
    let turn_origin = TurnOrigin::Subagent;
    let ctx = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &turn_origin,
        session_id: "agent-lessons-stage-no-persona-test",
        project_path: "",
        is_tool_loop: false,
        subagent_persona: None,
    });

    let raw = user_turn_body()?;
    let mut body = StageBody::new(Bytes::from(raw.clone()), "orig".to_string());
    AgentLessonsStage.apply(&ctx, &mut body).await;

    assert!(!body.filter_applied());
    assert!(body.filtered_stored().is_none());
    assert_eq!(body.body().as_ref(), raw.as_slice());
    Ok(())
}
