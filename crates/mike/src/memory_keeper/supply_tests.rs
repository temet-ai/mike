use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use anyhow::{Result, bail};
use chrono::Duration;

use super::super::arm::{KeeperArm, arm_for};
use super::super::block::{KEEPER_MARKER_OPEN, KEEPER_SOURCE};
use super::super::resolve::ThreadFacts;
use super::super::snapshot::{KeeperSnapshot, ProjectThreads, ThreadEntry};
use super::*;

const ROOT: &str = "/work/app";

fn now() -> DateTime<Utc> {
    DateTime::<Utc>::UNIX_EPOCH + Duration::seconds(1_794_225_600)
}

fn entry(slug: &str, linked: Option<Uuid>, status: &str) -> ThreadEntry {
    ThreadEntry {
        thread_id: 1,
        facts: ThreadFacts {
            slug: slug.to_owned(),
            title: format!("{slug} title"),
            goal: format!("{slug} goal"),
            status: status.to_owned(),
            branch: None,
            worktree: None,
            last_session_uuid: None,
            last_checkpoint_at: now() - Duration::days(1),
            linked_sessions: linked.into_iter().collect::<BTreeSet<_>>(),
            checkout_dirs: BTreeSet::new(),
        },
        text: Arc::from(format!("Work thread `{slug}` card text").as_str()),
        content_hash: format!("hash-{slug}"),
    }
}

fn snapshot(threads: Vec<ThreadEntry>) -> KeeperSnapshot {
    let mut projects = HashMap::new();
    projects.insert(7, Arc::new(ProjectThreads { threads }));
    KeeperSnapshot {
        generation: 1,
        projects,
        roots: vec![(ROOT.to_owned(), 7)],
    }
}

fn session_in(arm: KeeperArm) -> Result<Uuid> {
    for n in 0..1000_u128 {
        let id = Uuid::from_u128(n + 1);
        if arm_for(&id.to_string()) == arm {
            return Ok(id);
        }
    }
    bail!("no session in arm {arm:?} among 1000 ids")
}

fn request(session: Uuid, prompt: Option<&str>) -> SupplyRequest<'_> {
    SupplyRequest {
        session,
        project_path: ROOT,
        prompt,
        cwd: None,
        branch: None,
        recent: Vec::new(),
        carrier: Carrier::Tether,
        now: now(),
    }
}

#[test]
fn supply_continue_prompt_with_linked_thread_returns_its_card_in_the_wrapper() -> Result<()> {
    let me = session_in(KeeperArm::Keeper)?;
    let snap = snapshot(vec![
        entry("alpha", Some(me), "implementing"),
        entry("beta", None, "active"),
    ]);
    let out = supply(
        &snap,
        &request(me, Some("can we continue with the open epic")),
    );
    let Some(block) = out.block else {
        bail!("expected a block")
    };
    assert!(block.text.starts_with(&format!(
        "<system-reminder source=\"{KEEPER_SOURCE}\">\n{KEEPER_MARKER_OPEN}"
    )));
    assert!(block.text.contains("Work thread `alpha` card text"));
    let Some(record) = out.record else {
        bail!("expected a record")
    };
    assert_eq!(record.thread_slug.as_deref(), Some("alpha"));
    assert_eq!(record.resolution, "one");
    assert_eq!(record.content_hash.as_deref(), Some("hash-alpha"));
    assert_eq!(record.thread_id, Some(1));
    assert_eq!(record.block_hash.as_deref(), Some(block.hash.as_str()));
    assert_eq!(record.card_text.as_deref(), Some(block.text.as_str()));
    assert!(record.score.is_some_and(|s| s > 0.0));
    assert!((record.chance - 0.8).abs() < 1e-9);
    Ok(())
}

#[test]
fn supply_control_arm_session_gets_no_block_but_is_recorded() -> Result<()> {
    let me = session_in(KeeperArm::Control)?;
    let snap = snapshot(vec![entry("alpha", Some(me), "implementing")]);
    let out = supply(&snap, &request(me, Some("continue")));
    assert!(out.block.is_none());
    let Some(record) = out.record else {
        bail!("expected a record")
    };
    assert_eq!(record.arm, KeeperArm::Control);
    assert_eq!(record.resolution, "one");
    Ok(())
}

#[test]
fn supply_two_linked_threads_return_the_several_list_with_references() -> Result<()> {
    let me = session_in(KeeperArm::Keeper)?;
    let snap = snapshot(vec![
        entry("alpha", Some(me), "implementing"),
        entry("beta", Some(me), "review"),
    ]);
    let out = supply(&snap, &request(me, Some("where are we")));
    let Some(block) = out.block else {
        bail!("expected a block")
    };
    assert!(
        block
            .text
            .contains("- alpha (implementing): alpha goal [thread:alpha]")
    );
    assert!(
        block
            .text
            .contains("- beta (review): beta goal [thread:beta]")
    );
    assert_eq!(out.record.map(|r| r.resolution), Some("several"));
    Ok(())
}

#[test]
fn supply_unknown_project_returns_nothing_and_records_nothing() -> Result<()> {
    let me = session_in(KeeperArm::Keeper)?;
    let snap = snapshot(vec![entry("alpha", Some(me), "implementing")]);
    let mut req = request(me, Some("continue"));
    req.project_path = "/elsewhere";
    assert_eq!(supply(&snap, &req), Supply::nothing());
    Ok(())
}

#[test]
fn supply_no_prompt_with_linked_thread_returns_its_card() -> Result<()> {
    let me = session_in(KeeperArm::Keeper)?;
    let snap = snapshot(vec![entry("alpha", Some(me), "implementing")]);
    let out = supply(&snap, &request(me, None));
    assert!(out.block.is_some_and(|b| b.text.contains("`alpha`")));
    Ok(())
}

#[test]
fn supply_closed_thread_is_not_supplied() -> Result<()> {
    let me = session_in(KeeperArm::Keeper)?;
    let snap = snapshot(vec![entry("alpha", Some(me), "done")]);
    let out = supply(&snap, &request(me, Some("continue")));
    assert!(out.block.is_none());
    assert_eq!(out.record.map(|r| r.resolution), Some("none"));
    Ok(())
}

#[test]
fn supply_same_snapshot_and_prompt_give_byte_identical_blocks() -> Result<()> {
    let me = session_in(KeeperArm::Keeper)?;
    let snap = snapshot(vec![
        entry("alpha", Some(me), "implementing"),
        entry("beta", Some(me), "review"),
    ]);
    let first = supply(&snap, &request(me, Some("where are we")));
    let second = supply(&snap, &request(me, Some("where are we")));
    assert_eq!(first.block, second.block);
    Ok(())
}
