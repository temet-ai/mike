use bytes::Bytes;

use super::session_lessons_stage::SessionLessonsStage;
use super::stage_test_support::{StageCtxFixture, make_ctx, user_turn_body};
use crate::extract::request::TurnOrigin;
use crate::feature_toggles::ResolvedToggles;
use crate::filter::FilterConfig;
use crate::lesson_worker::LessonSnapshot;
use crate::pipeline::{RequestStage, StageBody};

#[tokio::test]
async fn session_lessons_enabled_requires_toggle_and_primary_turn() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("session-lessons-stage-enabled-test");
    let mut toggles_on: ResolvedToggles = (*resolved).clone();
    toggles_on.reminders_inject_lessons = true;
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
    assert!(SessionLessonsStage.enabled(&ctx_on));

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
    assert!(!SessionLessonsStage.enabled(&ctx_toggle_off));

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
    assert!(SessionLessonsStage.enabled(&ctx_tool_loop));

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
    assert!(!SessionLessonsStage.enabled(&ctx_wrong_origin));
    Ok(())
}

#[tokio::test]
async fn session_lessons_apply_injects_from_snapshot_and_records_manifest() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("session-lessons-stage-apply-test");
    let mut toggles_on: ResolvedToggles = (*resolved).clone();
    toggles_on.reminders_inject_lessons = true;
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::from_pairs([(
        "my-project".to_string(),
        vec![(
            "correction".to_string(),
            "always use make targets".to_string(),
        )],
    )]);
    let turn_origin = TurnOrigin::PrimaryFirst;
    let ctx = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &toggles_on,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &turn_origin,
        session_id: "session-lessons-stage-apply-test",
        project_path: "my-project",
        is_tool_loop: false,
        subagent_persona: None,
    });

    let raw = serde_json::to_vec(
        &serde_json::json!({"messages":[{"role":"user","content":[{"type":"text","text":"Use make targets"}]}]}),
    )?;
    let mut body = StageBody::new(Bytes::from(raw), "orig".to_string());
    SessionLessonsStage.apply(&ctx, &mut body).await;

    assert!(body.filter_applied());
    assert!(String::from_utf8_lossy(body.body()).contains("always use make targets"));
    let parts = body.into_parts();
    let manifest = parts.manifest.finish("orig".to_string());
    assert_eq!(manifest.stages.len(), 1);
    assert_eq!(manifest.stages[0].stage, "session_lessons");
    Ok(())
}

#[tokio::test]
async fn session_lessons_apply_leaves_body_untouched_when_snapshot_empty() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("session-lessons-stage-empty-test");
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::Primary;
    let ctx = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &turn_origin,
        session_id: "session-lessons-stage-empty-test",
        project_path: "unseeded-project",
        is_tool_loop: false,
        subagent_persona: None,
    });

    let raw = user_turn_body()?;
    let mut body = StageBody::new(Bytes::from(raw.clone()), "orig".to_string());
    SessionLessonsStage.apply(&ctx, &mut body).await;

    assert!(!body.filter_applied());
    assert!(body.filtered_stored().is_none());
    assert_eq!(body.body().as_ref(), raw.as_slice());
    Ok(())
}

#[tokio::test]
async fn session_lessons_enabled_only_on_session_own_lane_without_agent_id() -> anyhow::Result<()> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("session_lessons-own-lane-test");
    let mut toggles_on: ResolvedToggles = (*resolved).clone();
    toggles_on.reminders_inject_lessons = true;
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
    assert!(SessionLessonsStage.enabled(&own_lane));

    let mut agent_on_primary = make_ctx(fixture(&primary));
    agent_on_primary.has_agent_id = true;
    assert!(!SessionLessonsStage.enabled(&agent_on_primary));

    let subagent_lane = make_ctx(fixture(&subagent));
    assert!(!SessionLessonsStage.enabled(&subagent_lane));
    Ok(())
}
