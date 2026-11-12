use std::sync::Arc;

use super::RemindersOutcome;
use super::git_read_hint::{
    GitReadHintStage, hand_over, hint_reminder, is_git_read_command, last_turn_read_git,
};
use super::stage_test_support::{StageCtxFixture, make_ctx};
use crate::extract::request::TurnOrigin;
use crate::feature_toggles::ResolvedToggles;
use crate::filter::FilterConfig;
use crate::lesson_worker::LessonSnapshot;
use crate::pipeline::RequestStage;
use crate::store::models::RequestId;

fn body_after_bash(command: &str) -> Vec<u8> {
    serde_json::json!({
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "go"}]},
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": command}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "ok"}
            ]}
        ]
    })
    .to_string()
    .into_bytes()
}

#[test]
fn git_status_diff_and_log_count_as_reads() {
    for command in [
        "git status",
        "git status --short",
        "git diff HEAD~1",
        "git log --oneline -5",
        "git -C /repo status",
        "cd x && git diff | head",
    ] {
        assert!(is_git_read_command(command), "{command}");
    }
}

#[test]
fn git_commands_that_change_state_are_not_reads() {
    for command in [
        "git commit -m x",
        "git add -A",
        "git checkout main",
        "git push origin main",
        "git stash",
        "ls -la",
        "echo git status",
    ] {
        assert!(!is_git_read_command(command), "{command}");
    }
}

#[test]
fn last_turn_read_git_reports_message_count_for_a_git_read() {
    assert_eq!(last_turn_read_git(&body_after_bash("git status")), Some(3));
    assert_eq!(
        last_turn_read_git(&body_after_bash("git commit -m x")),
        None
    );
    assert_eq!(last_turn_read_git(b"not json"), None);
}

#[test]
fn hint_is_appended_after_a_git_read_then_withheld_inside_the_window() {
    let body = body_after_bash("git status");
    let first = hint_reminder("hint-window-a", &body, RequestId::next());
    let RemindersOutcome::Appended { reminder_text, .. } = first else {
        panic!("expected the hint to be appended");
    };
    assert!(reminder_text.contains("what_changed"));
    assert!(reminder_text.contains("view=changes"));
    let second = hint_reminder("hint-window-a", &body, RequestId::next());
    assert!(matches!(second, RemindersOutcome::Unchanged));
}

#[test]
fn hint_returns_only_after_a_compaction() {
    let body = body_after_bash("git status");
    let hint = || hint_reminder("hint-window-b", &body, RequestId::next());
    crate::explained::observe("hint-window-b", 500);
    assert!(matches!(hint(), RemindersOutcome::Appended { .. }));
    crate::explained::observe("hint-window-b", 900);
    assert!(matches!(hint(), RemindersOutcome::Unchanged));
    crate::explained::observe("hint-window-b", 4);
    assert!(matches!(hint(), RemindersOutcome::Appended { .. }));
}

#[test]
fn no_hint_after_a_state_changing_git_command() {
    let body = body_after_bash("git commit -m x");
    let outcome = hint_reminder("hint-window-c", &body, RequestId::next());
    assert!(matches!(outcome, RemindersOutcome::Unchanged));
}

async fn enabled_with_change_record(on: bool) -> anyhow::Result<bool> {
    let state =
        crate::state::AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let mut toggles = (**state.feature_toggle_overlay.load()).clone();
    toggles.change_record = on;
    state.feature_toggle_overlay.store(Arc::new(toggles));
    let resolved: ResolvedToggles = (*state.resolve_session_toggles("hint-test")).clone();
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let origin = TurnOrigin::Primary;
    let ctx = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &origin,
        session_id: "hint-test",
        project_path: "proj",
        is_tool_loop: true,
        subagent_persona: None,
    });
    Ok(GitReadHintStage.enabled(&ctx))
}

#[tokio::test]
async fn stage_runs_on_a_tool_loop_request_only_with_change_record_on() -> anyhow::Result<()> {
    assert!(enabled_with_change_record(true).await?);
    assert!(!enabled_with_change_record(false).await?);
    Ok(())
}

async fn state_with(change_record: bool, served: bool) -> anyhow::Result<crate::state::AppState> {
    let state =
        crate::state::AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let mut toggles = (**state.feature_toggle_overlay.load()).clone();
    toggles.change_record = change_record;
    toggles.client_serves_git_read_hint = served;
    state.feature_toggle_overlay.store(Arc::new(toggles));
    Ok(state)
}

async fn reason_after_git_read(state: &crate::state::AppState, session_id: &str) -> String {
    let resolved: ResolvedToggles = (*state.resolve_session_toggles(session_id)).clone();
    let filter_cfg = FilterConfig::default();
    let snapshot = LessonSnapshot::default();
    let origin = TurnOrigin::Primary;
    let ctx = make_ctx(StageCtxFixture {
        state,
        session_toggles: &resolved,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &snapshot,
        turn_origin: &origin,
        session_id,
        project_path: "proj",
        is_tool_loop: true,
        subagent_persona: None,
    });
    let mut body = crate::pipeline::StageBody::new(
        bytes::Bytes::from(body_after_bash("git status")),
        "orig".into(),
    );
    GitReadHintStage.apply(&ctx, &mut body).await;
    let manifest = body.into_parts().manifest.finish("orig".to_string());
    manifest
        .stages
        .first()
        .map(|s| s.reason_code.clone())
        .unwrap_or_default()
}

#[tokio::test]
async fn a_client_handed_the_hint_gives_it_and_the_stage_stands_aside() -> anyhow::Result<()> {
    let state = state_with(true, true).await?;
    let Some(block) = hand_over(&state, "hint-client-a").await else {
        anyhow::bail!("the client was not handed the hint");
    };
    assert_eq!(block.stage, "git_read_hint");
    assert!(block.text.contains("what_changed"));
    assert_eq!(
        reason_after_git_read(&state, "hint-client-a").await,
        "git_read_hint_served_by_client"
    );
    assert_eq!(
        reason_after_git_read(&state, "hint-client-b").await,
        "git_read_hint_injected",
        "a session whose client was not handed it still gets mike's"
    );
    Ok(())
}

#[tokio::test]
async fn nothing_is_handed_over_while_either_toggle_is_off() -> anyhow::Result<()> {
    assert!(
        hand_over(&state_with(true, false).await?, "hint-client-c")
            .await
            .is_none()
    );
    assert!(
        hand_over(&state_with(false, true).await?, "hint-client-d")
            .await
            .is_none()
    );
    assert_ne!(
        reason_after_git_read(&state_with(true, false).await?, "hint-client-c").await,
        "git_read_hint_served_by_client"
    );
    Ok(())
}
