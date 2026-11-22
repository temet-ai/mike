use anyhow::{Context, bail};
use uuid::Uuid;

use super::{expire_unused, keep_lanes_in_use, restore_stored, stored_identity, sweep};
use crate::injected_replay::{InjectedBlockRegistry, ReplayReport};
use crate::reminders::appender::append_block_to_last_user_message;
use crate::store::models::RequestId;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::proxy_repo::ProxyPersist as _;
use crate::store::persistence::test_support::TestDb;

const LESSON: &str = "<system-reminder source=\"proxy-lessons\">\nlesson one\n</system-reminder>";

fn first_turn() -> Vec<u8> {
    br#"{"model":"m","thread":{"type":"create"},"messages":[{"role":"user","content":[{"type":"text","text":"do it","cache_control":{"type":"ephemeral","ttl":"1h"}}]}]}"#.to_vec()
}

fn continuation() -> Vec<u8> {
    br#"{"model":"m","thread":{"type":"continue","previous_message_id":"msg_1"},"messages":[{"role":"user","content":[{"type":"text","text":"keep going","cache_control":{"type":"ephemeral","ttl":"1h"}}]}]}"#.to_vec()
}

fn full_resend() -> Vec<u8> {
    br#"{"model":"m","thread":{"type":"create"},"messages":[{"role":"user","content":[{"type":"text","text":"do it"}]},{"role":"assistant","content":[{"type":"text","text":"done"}]},{"role":"user","content":[{"type":"text","text":"keep going"}]},{"role":"assistant","content":[{"type":"text","text":"more"}]},{"role":"user","content":[{"type":"text","text":"after the gap","cache_control":{"type":"ephemeral","ttl":"1h"}}]}]}"#.to_vec()
}

async fn shared_handle() -> anyhow::Result<Option<PersistHandle>> {
    let Some(factory) = TestDb::bootstrap().await? else {
        return Ok(None);
    };
    let db = factory.acquire().await?;
    Ok(Some(PersistHandle::connect(&db.database_url).await?))
}

async fn record_injection(
    persist: &PersistHandle,
    machine: Uuid,
    registry: &InjectedBlockRegistry,
    lane: &str,
) -> anyhow::Result<()> {
    let first = first_turn();
    let sent = append_block_to_last_user_message(&first, LESSON, RequestId::next())
        .context("the lesson appends to the first turn")?;
    let Some(captured) = registry.capture(lane, &first, &sent) else {
        bail!("the injected lesson is captured");
    };
    persist
        .proxy()
        .insert_injected_replay_record(
            machine,
            lane,
            stored_identity(captured.identity),
            captured.tail(),
        )
        .await?;
    Ok(())
}

async fn minutes_pass(persist: &PersistHandle, machine: Uuid, minutes: i32) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE overrides.injected_replay_records
            SET recorded_at = recorded_at - make_interval(mins => $2)
          WHERE machine_uuid = $1",
    )
    .bind(machine)
    .bind(minutes)
    .execute(persist.pool())
    .await?;
    Ok(())
}

async fn remove_rows(persist: &PersistHandle, machine: Uuid) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM overrides.injected_replay_records WHERE machine_uuid = $1")
        .bind(machine)
        .execute(persist.pool())
        .await?;
    Ok(())
}

fn replayed(
    registry: &InjectedBlockRegistry,
    lane: &str,
    body: &[u8],
) -> anyhow::Result<(Vec<u8>, ReplayReport)> {
    let Some((out, report)) = registry.replay(lane, body) else {
        bail!("lane {lane} holds no records, so the full re-send goes out without the lesson");
    };
    Ok((out.to_vec(), report))
}

fn fresh_lane() -> String {
    format!("{}:primary", Uuid::new_v4())
}

#[tokio::test]
async fn restart_after_a_gap_replays_a_block_whose_lane_only_sent_continuations()
-> anyhow::Result<()> {
    let Some(persist) = shared_handle().await? else {
        return Ok(());
    };
    let (machine, lane) = (Uuid::new_v4(), fresh_lane());
    let before_restart = InjectedBlockRegistry::default();
    record_injection(&persist, machine, &before_restart, &lane).await?;
    let (sent_before_restart, _) = replayed(&before_restart, &lane, &full_resend())?;
    let _ = before_restart.take_lanes_in_use();

    minutes_pass(&persist, machine, 50).await?;
    let _ = before_restart.replay(&lane, &continuation());
    sweep(&persist, machine, &before_restart).await;
    minutes_pass(&persist, machine, 30).await?;

    let after_restart = InjectedBlockRegistry::default();
    restore_stored(&persist, machine, &after_restart).await?;
    let (sent_after_restart, report) = replayed(&after_restart, &lane, &full_resend())?;

    assert_eq!(report.replayed_indices, vec![0]);
    assert_eq!(
        String::from_utf8_lossy(&sent_after_restart),
        String::from_utf8_lossy(&sent_before_restart),
        "the full re-send after the restart carries the bytes the cache holds"
    );
    remove_rows(&persist, machine).await
}

#[tokio::test]
async fn restart_soon_after_a_use_replays_once_shutdown_kept_the_lane() -> anyhow::Result<()> {
    let Some(persist) = shared_handle().await? else {
        return Ok(());
    };
    let (machine, lane) = (Uuid::new_v4(), fresh_lane());
    let before_restart = InjectedBlockRegistry::default();
    record_injection(&persist, machine, &before_restart, &lane).await?;
    sweep(&persist, machine, &before_restart).await;

    minutes_pass(&persist, machine, 55).await?;
    let (sent_before_restart, _) = replayed(&before_restart, &lane, &full_resend())?;
    keep_lanes_in_use(&persist, machine, &before_restart).await?;
    minutes_pass(&persist, machine, 10).await?;

    let after_restart = InjectedBlockRegistry::default();
    restore_stored(&persist, machine, &after_restart).await?;
    let (sent_after_restart, _) = replayed(&after_restart, &lane, &full_resend())?;

    assert_eq!(sent_after_restart, sent_before_restart);
    remove_rows(&persist, machine).await
}

#[tokio::test]
async fn expire_unused_lane_forgets_its_records_in_memory_too() -> anyhow::Result<()> {
    let Some(persist) = shared_handle().await? else {
        return Ok(());
    };
    let (machine, lane) = (Uuid::new_v4(), fresh_lane());
    let registry = InjectedBlockRegistry::default();
    record_injection(&persist, machine, &registry, &lane).await?;

    minutes_pass(&persist, machine, 61).await?;
    let expired = expire_unused(&persist, machine, &registry).await?;

    assert_eq!(expired, 1);
    assert_eq!(registry.records_in(&lane), 0);
    assert!(registry.replay(&lane, &full_resend()).is_none());
    remove_rows(&persist, machine).await
}
