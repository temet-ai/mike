use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use anyhow::{Result, bail};
use bytes::Bytes;
use chrono::Utc;
use serde_json::json;

use super::super::arm::{KeeperArm, arm_for};
use super::super::resolve::ThreadFacts;
use super::super::served::mark_served;
use super::super::snapshot::{KeeperSnapshot, ProjectThreads, ThreadEntry};
use super::*;
use crate::extract::request::TurnOrigin;
use crate::filter::FilterConfig;
use crate::lesson_worker::LessonSnapshot;
use crate::pipeline::StageBody;
use crate::reminders::stage_test_support::{StageCtxFixture, make_ctx};

const ROOT: &str = "/work/stage-app";

fn session_in(arm: KeeperArm) -> Result<Uuid> {
    for n in 1..2000_u128 {
        let id = Uuid::from_u128(n * 7919);
        if arm_for(&id.to_string()) == arm {
            return Ok(id);
        }
    }
    bail!("no session in arm {arm:?}")
}

fn snapshot_with_linked(session: Uuid) -> KeeperSnapshot {
    let entry = ThreadEntry {
        thread_id: 9,
        facts: ThreadFacts {
            slug: "alpha".to_owned(),
            title: "Alpha".to_owned(),
            goal: "alpha goal".to_owned(),
            status: "implementing".to_owned(),
            branch: None,
            worktree: None,
            last_session_uuid: None,
            last_checkpoint_at: Utc::now() - chrono::Duration::hours(2),
            linked_sessions: BTreeSet::from([session]),
            checkout_dirs: BTreeSet::new(),
        },
        text: Arc::from("Work thread `alpha` (implementing) [thread:alpha]"),
        content_hash: "h".to_owned(),
    };
    let mut projects = HashMap::new();
    projects.insert(
        3,
        Arc::new(ProjectThreads {
            threads: vec![entry],
        }),
    );
    KeeperSnapshot {
        generation: 1,
        projects,
        roots: vec![(ROOT.to_owned(), 3)],
    }
}

fn body(prompt: &str) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&json!({
        "model": "claude-sonnet-4",
        "messages": [{"role": "user", "content": [{"type": "text", "text": prompt}]}],
    }))?)
}

async fn run_stage(session: Uuid, prompt: &str) -> Result<StageBody> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    let state = crate::state::AppState::new(config).await?;
    state
        .keeper_snapshot
        .store(Arc::new(snapshot_with_linked(session)));
    let id = session.to_string();
    let toggles = state.resolve_session_toggles(&id);
    let filter_cfg = FilterConfig::default();
    let lessons = LessonSnapshot::default();
    let origin = TurnOrigin::Primary;
    let ctx = make_ctx(StageCtxFixture {
        state: &state,
        session_toggles: &toggles,
        filter_cfg: &filter_cfg,
        lesson_snapshot: &lessons,
        turn_origin: &origin,
        session_id: &id,
        project_path: ROOT,
        is_tool_loop: false,
        subagent_persona: None,
    });
    let mut stage_body = StageBody::new(Bytes::from(body(prompt)?), "orig".to_owned());
    MemoryKeeperStage.apply(&ctx, &mut stage_body).await;
    Ok(stage_body)
}

#[tokio::test]
async fn apply_continue_prompt_appends_the_card_to_the_last_user_turn() -> Result<()> {
    let session = session_in(KeeperArm::Keeper)?;
    let out = run_stage(session, "can we continue with the open epic").await?;
    assert!(out.filter_applied());
    let text = String::from_utf8_lossy(out.body()).into_owned();
    assert!(text.contains("source=\\\"memory-keeper\\\""));
    assert!(text.contains("Work thread `alpha`"));
    Ok(())
}

#[tokio::test]
async fn apply_same_prompt_twice_gives_byte_identical_bodies() -> Result<()> {
    let session = session_in(KeeperArm::Keeper)?;
    let first = run_stage(session, "where are we").await?;
    let second = run_stage(session, "where are we").await?;
    assert_eq!(first.body(), second.body());
    Ok(())
}

#[tokio::test]
async fn apply_body_already_carrying_a_card_marker_is_left_unchanged() -> Result<()> {
    let session = session_in(KeeperArm::Keeper)?;
    let out = run_stage(session, "continue [keeper:abcdef012345] earlier card").await?;
    assert!(!out.filter_applied());
    Ok(())
}

#[tokio::test]
async fn apply_session_served_by_a_client_is_left_unchanged() -> Result<()> {
    let session = session_in(KeeperArm::Keeper)?;
    mark_served(&session.to_string());
    let out = run_stage(session, "continue").await?;
    assert!(!out.filter_applied());
    Ok(())
}

#[tokio::test]
async fn apply_control_arm_session_is_left_unchanged() -> Result<()> {
    let session = session_in(KeeperArm::Control)?;
    let out = run_stage(session, "continue").await?;
    assert!(!out.filter_applied());
    Ok(())
}

#[test]
fn carries_marker_finds_the_marker_only_when_present() {
    assert!(carries_marker(b"x [keeper:0123] y"));
    assert!(!carries_marker(b"no card here"));
}
