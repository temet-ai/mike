use bytes::Bytes;

use super::stage_test_support::{StageCtxFixture, make_ctx, user_turn_body};
use super::workspace_briefing::WorkspaceBriefingStage;
use crate::extract::request::TurnOrigin;
use crate::feature_toggles::ResolvedToggles;
use crate::filter::FilterConfig;
use crate::lesson_worker::LessonSnapshot;
use crate::pipeline::{RequestStage, StageBody};

async fn in_memory_state() -> anyhow::Result<crate::state::AppState> {
    crate::state::AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await
}

#[tokio::test]
async fn workspace_briefing_enabled_requires_toggle_primary_turn_and_no_tool_loop()
-> anyhow::Result<()> {
    let state = in_memory_state().await?;
    let mut resolved: ResolvedToggles =
        (*state.resolve_session_toggles("workspace-briefing-enabled-test")).clone();
    resolved.workspace_briefing = true;
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let primary = TurnOrigin::Primary;

    let ctx_on = make_ctx(StageCtxFixture {
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
    assert!(WorkspaceBriefingStage.enabled(&ctx_on));

    let mut toggles_off: ResolvedToggles = resolved.clone();
    toggles_off.workspace_briefing = false;
    let ctx_toggle_off = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &toggles_off,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &primary,
        session_id: "s",
        project_path: "proj",
        is_tool_loop: false,
        subagent_persona: None,
    });
    assert!(!WorkspaceBriefingStage.enabled(&ctx_toggle_off));

    let ctx_tool_loop = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &primary,
        session_id: "s",
        project_path: "proj",
        is_tool_loop: true,
        subagent_persona: None,
    });
    assert!(!WorkspaceBriefingStage.enabled(&ctx_tool_loop));

    let subagent = TurnOrigin::Subagent;
    let ctx_wrong_origin = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &subagent,
        session_id: "s",
        project_path: "proj",
        is_tool_loop: false,
        subagent_persona: None,
    });
    assert!(!WorkspaceBriefingStage.enabled(&ctx_wrong_origin));
    Ok(())
}

#[tokio::test]
async fn workspace_briefing_enabled_for_a_forked_session_first_turn() -> anyhow::Result<()> {
    let state = in_memory_state().await?;
    let mut resolved: ResolvedToggles =
        (*state.resolve_session_toggles("workspace-briefing-fork-test")).clone();
    resolved.workspace_briefing = true;
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let primary_first = TurnOrigin::PrimaryFirst;

    let ctx = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &primary_first,
        session_id: "s",
        project_path: "proj",
        is_tool_loop: false,
        subagent_persona: None,
    });
    assert!(WorkspaceBriefingStage.enabled(&ctx));
    Ok(())
}

#[tokio::test]
async fn workspace_briefing_apply_records_no_persist_when_no_db_is_configured() -> anyhow::Result<()>
{
    let state = in_memory_state().await?;
    let resolved = state.resolve_session_toggles("workspace-briefing-no-persist-test");
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::PrimaryFirst;
    let ctx = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &turn_origin,
        session_id: "workspace-briefing-no-persist-test",
        project_path: "my-project",
        is_tool_loop: false,
        subagent_persona: None,
    });

    let raw = user_turn_body()?;
    let mut body = StageBody::new(Bytes::from(raw.clone()), "orig".to_string());
    WorkspaceBriefingStage.apply(&ctx, &mut body).await;

    assert!(!body.filter_applied());
    assert_eq!(body.body().as_ref(), raw.as_slice());
    let manifest = body.into_parts().manifest.finish("orig".to_string());
    assert_eq!(manifest.stages.len(), 1);
    assert_eq!(manifest.stages[0].stage, "workspace_briefing");
    assert_eq!(
        manifest.stages[0].reason_code, "workspace_briefing_no_persist",
        "a state with no persistence handle must report the no-persist reason code"
    );
    Ok(())
}

#[tokio::test]
async fn workspace_briefing_apply_short_circuits_when_already_delivered_this_process()
-> anyhow::Result<()> {
    let state = in_memory_state().await?;
    let session_id = "workspace-briefing-already-delivered-test";
    state
        .workspace_briefing_delivered
        .insert(session_id.to_string());
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
    WorkspaceBriefingStage.apply(&ctx, &mut body).await;

    assert!(!body.filter_applied());
    let manifest = body.into_parts().manifest.finish("orig".to_string());
    assert_eq!(
        manifest.stages[0].reason_code, "workspace_briefing_already_delivered",
        "the once-only memo must short-circuit before any persistence lookup"
    );
    Ok(())
}

async fn briefing_reason_for(
    state: &crate::state::AppState,
    session_id: &str,
    first_text: &str,
) -> anyhow::Result<String> {
    let resolved = state.resolve_session_toggles(session_id);
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let turn_origin = TurnOrigin::PrimaryFirst;
    let ctx = make_ctx(StageCtxFixture {
        state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &turn_origin,
        session_id,
        project_path: "my-project",
        is_tool_loop: false,
        subagent_persona: None,
    });
    let raw = serde_json::to_vec(&serde_json::json!({
        "model": "claude-sonnet-4",
        "messages": [{"role": "user", "content": [{"type": "text", "text": first_text}]}],
    }))?;
    let mut body = StageBody::new(Bytes::from(raw), "orig".to_string());
    WorkspaceBriefingStage.apply(&ctx, &mut body).await;
    let manifest = body.into_parts().manifest.finish("orig".to_string());
    Ok(manifest.stages[0].reason_code.clone())
}

#[tokio::test]
async fn workspace_briefing_stands_aside_when_the_client_briefing_is_in_the_request()
-> anyhow::Result<()> {
    let state = in_memory_state().await?;
    let session_id = "workspace-briefing-client-delivered-test";
    state
        .workspace_briefing_served
        .insert(session_id.to_string());
    let delivered =
        "<system-reminder source=\"proxy-workspace-briefing\">\nroster\n</system-reminder>";
    assert_eq!(
        briefing_reason_for(&state, session_id, delivered).await?,
        "workspace_briefing_already_delivered"
    );
    Ok(())
}

#[tokio::test]
async fn workspace_briefing_keeps_its_stage_when_the_client_answer_never_arrived()
-> anyhow::Result<()> {
    let state = in_memory_state().await?;
    let session_id = "workspace-briefing-client-missed-test";
    state
        .workspace_briefing_served
        .insert(session_id.to_string());
    assert_ne!(
        briefing_reason_for(&state, session_id, "hello").await?,
        "workspace_briefing_already_delivered"
    );
    Ok(())
}

#[tokio::test]
async fn workspace_briefing_enabled_only_on_session_own_lane_without_agent_id() -> anyhow::Result<()>
{
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    let resolved = state.resolve_session_toggles("workspace_briefing-own-lane-test");
    let mut toggles_on: ResolvedToggles = (*resolved).clone();
    toggles_on.workspace_briefing = true;
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
    assert!(WorkspaceBriefingStage.enabled(&own_lane));

    let mut agent_on_primary = make_ctx(fixture(&primary));
    agent_on_primary.has_agent_id = true;
    assert!(!WorkspaceBriefingStage.enabled(&agent_on_primary));

    let subagent_lane = make_ctx(fixture(&subagent));
    assert!(!WorkspaceBriefingStage.enabled(&subagent_lane));
    Ok(())
}

#[tokio::test]
async fn workspace_briefing_apply_leaves_a_template_session_body_unchanged() -> anyhow::Result<()> {
    let state = in_memory_state().await?;
    let session_id = "workspace-briefing-template-session-test";
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
    WorkspaceBriefingStage.apply(&ctx, &mut body).await;

    assert!(!body.filter_applied());
    assert_eq!(body.body().as_ref(), raw.as_slice());
    let manifest = body.into_parts().manifest.finish("orig".to_string());
    assert_eq!(manifest.stages.len(), 1);
    assert_eq!(
        manifest.stages[0].reason_code,
        "workspace_briefing_template_session"
    );
    Ok(())
}
