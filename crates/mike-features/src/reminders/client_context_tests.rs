use std::sync::Arc;

use bytes::Bytes;
use uuid::Uuid;

use super::change_record_tail::ChangeRecordTailStage;
use super::client_context::{
    ContextBlock, briefing_blocks, findings_ready_for, prompt_context, reminders_served_by_client,
};

#[test]
fn the_briefing_hands_its_standing_protocol_over_as_a_fixed_block_of_its_own() {
    let protocol = crate::api::workspace::protocol_text(false);
    let briefing = crate::api::workspace::Briefing {
        identity: "Workspace roster.\n\nYou are session s.\n\n".to_owned(),
        roster: "1 other session(s) registered on this project:\n- peer [live]\n\n".to_owned(),
        protocol: true,
        fleet_seat: false,
    };
    let blocks = briefing_blocks(&briefing);
    let stages: Vec<_> = blocks.iter().map(|b| b.stage).collect();
    assert_eq!(
        stages,
        [
            "workspace_protocol",
            "workspace_identity",
            "workspace_briefing"
        ]
    );
    assert!(blocks[0].text.contains(&protocol));
    assert!(!blocks[1].text.contains(&protocol));
    assert_eq!(
        blocks[2].text,
        "<system-reminder source=\"proxy-workspace-briefing\">\n1 other session(s) registered on this project:\n- peer [live]\n</system-reminder>"
    );
}
use super::formatter::build_reminder_text;
use super::self_identity::build_self_identity_reminder_text;
use super::stage::{RemindersStage, SelfIdentityStage};
use super::stage_test_support::{
    StageCtxFixture, make_ctx, mk_finding, sample_request, user_turn_body,
};
use super::{RemindersConfig, RemindersOutcome, apply_reminders};
use crate::analysis::{AnalysisConfig, AnalyzerRegistry};
use crate::config::ProxyConfig;
use crate::extract::request::TurnOrigin;
use crate::filter::FilterConfig;
use crate::lesson_cache::LessonSnapshot;
use crate::pipeline::{RequestStage, StageBody};
use crate::state::AppState;
use crate::store::models::{Diagnostic, RequestId};
use crate::store::updater::run_analysis_and_notify;

async fn state_with_identity_toggle(on: bool) -> anyhow::Result<AppState> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    crate::state::serve_every_stage_for_tests(&state);
    let mut toggles = (**state.feature_toggle_overlay.load()).clone();
    toggles.self_identity_reminder = on;
    state.feature_toggle_overlay.store(Arc::new(toggles));
    Ok(state)
}

async fn apply_with_body(
    state: &AppState,
    session_id: &str,
    stage: &dyn RequestStage,
    raw: Vec<u8>,
) -> anyhow::Result<StageBody> {
    let resolved = state.resolve_session_toggles(session_id);
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let origin = TurnOrigin::PrimaryFirst;
    let ctx = make_ctx(StageCtxFixture {
        state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &origin,
        session_id,
        project_path: "proj",
        is_tool_loop: false,
        subagent_persona: None,
    });
    let mut body = StageBody::new(Bytes::from(raw), "orig".to_string());
    stage.apply(&ctx, &mut body).await;
    Ok(body)
}

async fn apply_with(
    state: &AppState,
    session_id: &str,
    stage: &dyn RequestStage,
) -> anyhow::Result<StageBody> {
    apply_with_body(state, session_id, stage, user_turn_body()?).await
}

fn body_carrying(text: &str) -> anyhow::Result<Vec<u8>> {
    Ok(serde_json::to_vec(&serde_json::json!({
        "model": "claude-sonnet-4",
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": text}, {"type": "text", "text": "q"}]},
        ],
    }))?)
}

fn reason_code(body: StageBody) -> String {
    let manifest = body.into_parts().manifest.finish("orig".to_string());
    manifest
        .stages
        .first()
        .map(|stage| stage.reason_code.clone())
        .unwrap_or_default()
}

#[tokio::test]
async fn prompt_context_identity_text_equals_the_stage_builder() -> anyhow::Result<()> {
    let state = state_with_identity_toggle(true).await?;
    let session = Uuid::new_v4();
    let id = session.to_string();
    let blocks = prompt_context(&state, session, &id, "/tmp/proj").await;
    assert_eq!(
        blocks,
        vec![ContextBlock {
            stage: "self_identity",
            text: build_self_identity_reminder_text(&id),
        }]
    );
    Ok(())
}

#[tokio::test]
async fn prompt_context_with_the_toggle_off_answers_nothing_and_marks_nothing() -> anyhow::Result<()>
{
    let state = state_with_identity_toggle(false).await?;
    let session = Uuid::new_v4();
    let id = session.to_string();
    assert!(
        prompt_context(&state, session, &id, "/tmp/proj")
            .await
            .is_empty()
    );
    assert!(!state.self_identity_served.contains(&id));
    Ok(())
}

#[tokio::test]
async fn prompt_context_with_client_serves_off_keeps_the_stage_in_mike() -> anyhow::Result<()> {
    let state = state_with_identity_toggle(true).await?;
    let mut toggles = (**state.feature_toggle_overlay.load()).clone();
    toggles.client_serves_self_identity = false;
    state.feature_toggle_overlay.store(Arc::new(toggles));
    let session = Uuid::new_v4();
    let id = session.to_string();
    assert!(
        prompt_context(&state, session, &id, "/tmp/proj")
            .await
            .is_empty()
    );
    assert!(!state.self_identity_served.contains(&id));
    let body = apply_with(&state, &id, &SelfIdentityStage).await?;
    assert!(
        body.filter_applied(),
        "mike still injects the identity text"
    );
    Ok(())
}

#[tokio::test]
async fn self_identity_stage_stands_aside_after_context_served() -> anyhow::Result<()> {
    let state = state_with_identity_toggle(true).await?;
    let session = Uuid::new_v4();
    let id = session.to_string();
    prompt_context(&state, session, &id, "/tmp/proj").await;
    let raw = body_carrying(&build_self_identity_reminder_text(&id))?;
    let body = apply_with_body(&state, &id, &SelfIdentityStage, raw).await?;
    assert!(!body.filter_applied());
    assert_eq!(reason_code(body), "self_identity_served_by_client");
    Ok(())
}

#[tokio::test]
async fn self_identity_stage_injects_when_the_client_answer_never_reached_the_request()
-> anyhow::Result<()> {
    let state = state_with_identity_toggle(true).await?;
    let session = Uuid::new_v4();
    let id = session.to_string();
    prompt_context(&state, session, &id, "/tmp/proj").await;
    let body = apply_with(&state, &id, &SelfIdentityStage).await?;
    assert!(body.filter_applied());
    assert_eq!(reason_code(body), "self_identity_injected");
    Ok(())
}

#[tokio::test]
async fn self_identity_stage_still_injects_for_a_session_that_never_asked() -> anyhow::Result<()> {
    let state = state_with_identity_toggle(true).await?;
    let asked = Uuid::new_v4();
    prompt_context(&state, asked, &asked.to_string(), "/tmp/proj").await;
    let other = Uuid::new_v4().to_string();
    let body = apply_with(&state, &other, &SelfIdentityStage).await?;
    assert!(body.filter_applied());
    assert_eq!(reason_code(body), "self_identity_injected");
    Ok(())
}

#[tokio::test]
async fn change_record_tail_stage_stands_aside_for_a_served_session() -> anyhow::Result<()> {
    let state = state_with_identity_toggle(false).await?;
    let id = Uuid::new_v4().to_string();
    state.change_record_tail_served.insert(id.clone());
    let body = apply_with(&state, &id, &ChangeRecordTailStage).await?;
    assert!(!body.filter_applied());
    assert_eq!(reason_code(body), "change_record_tail_served_by_client");
    Ok(())
}

async fn state_with_reminders(on: bool) -> anyhow::Result<AppState> {
    let state = AppState::new(ProxyConfig::for_tests_in_memory()).await?;
    crate::state::serve_every_stage_for_tests(&state);
    let mut toggles = (**state.feature_toggle_overlay.load()).clone();
    toggles.reminders = on;
    state.feature_toggle_overlay.store(Arc::new(toggles));
    Ok(state)
}

async fn seed_findings(state: &AppState, session_id: &str, raw_id: u64, findings: Vec<Diagnostic>) {
    let mut req = sample_request(RequestId::from_raw(raw_id), session_id);
    req.diagnostics = findings;
    state.store.write().await.insert(req);
}

fn findings_block(blocks: &[ContextBlock]) -> Option<&ContextBlock> {
    blocks.iter().find(|block| block.stage == "reminders")
}

async fn apply_reminders_stage(state: &AppState, session_id: &str) -> anyhow::Result<StageBody> {
    let mut toggles = (*state.resolve_session_toggles(session_id)).clone();
    toggles.reminders = true;
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let origin = TurnOrigin::Primary;
    let ctx = make_ctx(StageCtxFixture {
        state,
        session_toggles: &toggles,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &origin,
        session_id,
        project_path: "proj",
        is_tool_loop: false,
        subagent_persona: None,
    });
    let mut body = StageBody::new(Bytes::from(user_turn_body()?), "orig".to_string());
    RemindersStage.apply(&ctx, &mut body).await;
    Ok(body)
}

#[tokio::test]
async fn findings_block_text_equals_build_reminder_text_for_the_same_findings() -> anyhow::Result<()>
{
    let state = state_with_reminders(true).await?;
    let session = Uuid::new_v4();
    let id = session.to_string();
    let findings = vec![mk_finding(
        "comprehensive solution",
        "no MVP without ask",
        0,
    )];
    seed_findings(&state, &id, 41, findings.clone()).await;
    let blocks = prompt_context(&state, session, &id, "/tmp/proj").await;
    let expected = build_reminder_text(&findings)
        .ok_or_else(|| anyhow::anyhow!("findings must render a reminder"))?;
    let block = findings_block(&blocks).ok_or_else(|| anyhow::anyhow!("no reminders block"))?;
    assert_eq!(block.text, expected);
    Ok(())
}

#[tokio::test]
async fn findings_block_is_absent_when_the_reminders_toggle_is_off() -> anyhow::Result<()> {
    let state = state_with_reminders(false).await?;
    let session = Uuid::new_v4();
    let id = session.to_string();
    seed_findings(
        &state,
        &id,
        42,
        vec![mk_finding(
            "comprehensive solution",
            "no MVP without ask",
            0,
        )],
    )
    .await;
    let blocks = prompt_context(&state, session, &id, "/tmp/proj").await;
    assert!(findings_block(&blocks).is_none());
    assert!(!reminders_served_by_client(&id));
    Ok(())
}

#[tokio::test]
async fn reminders_stage_stands_aside_after_context_served() -> anyhow::Result<()> {
    let state = state_with_reminders(true).await?;
    let session = Uuid::new_v4();
    let id = session.to_string();
    seed_findings(
        &state,
        &id,
        43,
        vec![mk_finding(
            "comprehensive solution",
            "no MVP without ask",
            0,
        )],
    )
    .await;
    prompt_context(&state, session, &id, "/tmp/proj").await;
    let body = apply_reminders_stage(&state, &id).await?;
    assert!(!body.filter_applied());
    assert_eq!(reason_code(body), "reminders_served_by_client");
    Ok(())
}

#[tokio::test]
async fn reminders_stage_still_injects_for_a_session_that_never_asked() -> anyhow::Result<()> {
    let state = state_with_reminders(true).await?;
    let asked = Uuid::new_v4();
    seed_findings(
        &state,
        &asked.to_string(),
        44,
        vec![mk_finding("for now", "There is no 'later'", 0)],
    )
    .await;
    prompt_context(&state, asked, &asked.to_string(), "/tmp/proj").await;
    let other = Uuid::new_v4().to_string();
    seed_findings(
        &state,
        &other,
        45,
        vec![mk_finding(
            "comprehensive solution",
            "no MVP without ask",
            0,
        )],
    )
    .await;
    let body = apply_reminders_stage(&state, &other).await?;
    assert!(body.filter_applied());
    assert!(String::from_utf8_lossy(body.body()).contains("comprehensive solution"));
    assert_eq!(reason_code(body), "diagnostic_reminder_injected");
    Ok(())
}

#[tokio::test]
async fn apply_reminders_carries_only_the_findings_block() -> anyhow::Result<()> {
    let state = state_with_reminders(true).await?;
    let id = Uuid::new_v4().to_string();
    let findings = vec![mk_finding("for now", "There is no 'later'", 0)];
    seed_findings(&state, &id, 46, findings.clone()).await;
    let toggles = state.resolve_session_toggles(&id);
    let cfg = RemindersConfig {
        enabled: toggles.reminders,
        inject_lessons: toggles.reminders_inject_lessons,
        max_lessons_to_inject: toggles.reminders_max_lessons_to_inject,
    };
    let raw = user_turn_body()?;
    let push_deps = super::test_support::noop_push_deps();
    let outcome = apply_reminders(
        &raw,
        &id,
        &state.store,
        &cfg,
        RequestId::from_raw(1),
        &push_deps,
    )
    .await;
    let RemindersOutcome::Appended {
        body,
        reminder_text,
        source_finding_ids,
        source_request_id,
    } = outcome
    else {
        anyhow::bail!("expected the findings to be appended");
    };
    let expected = build_reminder_text(&findings)
        .ok_or_else(|| anyhow::anyhow!("findings must render a reminder"))?;
    assert_eq!(reminder_text, expected);
    assert_eq!(source_request_id, 46);
    assert_eq!(source_finding_ids, vec![0]);
    let text = String::from_utf8_lossy(&body);
    let escaped = serde_json::to_string(&expected)?;
    assert!(text.contains(escaped.trim_matches('"')));
    assert!(!text.contains("proxy-lessons"));
    Ok(())
}

#[tokio::test]
async fn findings_ready_is_false_while_the_latest_request_is_unanalysed() -> anyhow::Result<()> {
    let state = state_with_reminders(true).await?;
    let id = Uuid::new_v4().to_string();
    seed_findings(&state, &id, 9001, Vec::new()).await;
    assert!(!findings_ready_for(&state, &id).await);
    Ok(())
}

#[tokio::test]
async fn findings_ready_turns_true_once_analysis_completes_with_no_findings() -> anyhow::Result<()>
{
    let state = state_with_reminders(true).await?;
    let session = Uuid::new_v4();
    let id = session.to_string();
    seed_findings(&state, &id, 9002, Vec::new()).await;
    let registry = Arc::new(arc_swap::ArcSwap::from_pointee(
        AnalyzerRegistry::with_defaults(&AnalysisConfig::default())?,
    ));
    let (tx, _rx) = tokio::sync::broadcast::channel(8);
    state.store.write().await.set_analysis(registry, tx);
    run_analysis_and_notify(
        &state.store,
        RequestId::from_raw(9002),
        "plain text with nothing to flag",
    )
    .await;
    assert!(findings_ready_for(&state, &id).await);
    let blocks = prompt_context(&state, session, &id, "/tmp/proj").await;
    assert!(findings_block(&blocks).is_none());
    Ok(())
}
