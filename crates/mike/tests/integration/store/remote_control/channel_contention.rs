use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, SubsecRound, Utc};

use mike::store::persistence::PersistHandle;
use mike::store::persistence::remote_control::{
    PATH_HEARTBEAT, RemoteControlCall, RemoteControlSignals,
};

use crate::store::remote_control::support::{call_at, shared_handle, unique};

const EVENTS: &str = "/worker/events";

fn at(base: DateTime<Utc>, seconds: i64) -> DateTime<Utc> {
    base + Duration::seconds(seconds)
}

async fn last_seen(handle: &PersistHandle, cse_id: &str) -> anyhow::Result<DateTime<Utc>> {
    Ok(
        sqlx::query_scalar("SELECT last_seen_at FROM remote_control.channels WHERE cse_id = $1")
            .bind(cse_id)
            .fetch_one(handle.pool())
            .await?,
    )
}

async fn call_count(handle: &PersistHandle, cse_id: &str) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM remote_control.calls c
           JOIN remote_control.channels ch ON ch.id = c.channel_id
          WHERE ch.cse_id = $1",
    )
    .bind(cse_id)
    .fetch_one(handle.pool())
    .await?)
}

#[tokio::test]
async fn a_call_that_changes_nothing_records_while_the_channel_row_is_locked() -> anyhow::Result<()>
{
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_locked");
    let base = Utc::now().trunc_subsecs(6);
    handle
        .record_remote_control_call(RemoteControlCall {
            created_at: base,
            ..call_at(&cse_id, "POST", EVENTS)
        })
        .await?;

    let mut holder = handle.pool().begin().await?;
    sqlx::query("UPDATE remote_control.channels SET last_seen_at = last_seen_at WHERE cse_id = $1")
        .bind(&cse_id)
        .execute(&mut *holder)
        .await?;

    let recorded = tokio::time::timeout(
        StdDuration::from_secs(3),
        handle.record_remote_control_call(RemoteControlCall {
            created_at: at(base, 1),
            ..call_at(&cse_id, "POST", EVENTS)
        }),
    )
    .await;
    holder.rollback().await?;

    assert!(
        recorded.is_ok(),
        "a call that changes no channel state must not wait on the channel row lock"
    );
    recorded??;
    assert_eq!(call_count(&handle, &cse_id).await?, 2);
    assert_eq!(last_seen(&handle, &cse_id).await?, base);

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn last_seen_moves_only_past_the_debounce_and_heartbeats_always_move() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_debounce");
    let base = Utc::now().trunc_subsecs(6);
    for (seconds, path) in [(0, EVENTS), (5, EVENTS)] {
        handle
            .record_remote_control_call(RemoteControlCall {
                created_at: at(base, seconds),
                ..call_at(&cse_id, "POST", path)
            })
            .await?;
    }
    assert_eq!(last_seen(&handle, &cse_id).await?, base);

    handle
        .record_remote_control_call(RemoteControlCall {
            created_at: at(base, 11),
            ..call_at(&cse_id, "POST", EVENTS)
        })
        .await?;
    assert_eq!(last_seen(&handle, &cse_id).await?, at(base, 11));

    for seconds in [12, 13] {
        handle
            .record_remote_control_call(RemoteControlCall {
                created_at: at(base, seconds),
                signals: RemoteControlSignals {
                    idle_seconds: Some(1),
                    ..RemoteControlSignals::default()
                },
                ..call_at(&cse_id, "POST", PATH_HEARTBEAT)
            })
            .await?;
    }
    let heartbeat: DateTime<Utc> = sqlx::query_scalar(
        "SELECT last_heartbeat_at FROM remote_control.channels WHERE cse_id = $1",
    )
    .bind(&cse_id)
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(heartbeat, at(base, 13));
    assert_eq!(call_count(&handle, &cse_id).await?, 5);

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn a_status_change_is_written_and_logged_once_even_inside_the_debounce() -> anyhow::Result<()>
{
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_status");
    let base = Utc::now().trunc_subsecs(6);
    for (seconds, status) in [(0, "idle"), (1, "requires_action"), (2, "requires_action")] {
        handle
            .record_remote_control_call(RemoteControlCall {
                created_at: at(base, seconds),
                signals: RemoteControlSignals {
                    worker_status: Some(status),
                    ..RemoteControlSignals::default()
                },
                ..call_at(&cse_id, "PUT", "/worker")
            })
            .await?;
    }
    let status: String =
        sqlx::query_scalar("SELECT worker_status FROM remote_control.channels WHERE cse_id = $1")
            .bind(&cse_id)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(status, "requires_action");
    let transitions: Vec<(Option<String>, String)> = sqlx::query_as(
        "SELECT t.from_status, t.to_status FROM remote_control.worker_status_transitions t
           JOIN remote_control.channels ch ON ch.id = t.channel_id
          WHERE ch.cse_id = $1 ORDER BY t.observed_at",
    )
    .bind(&cse_id)
    .fetch_all(handle.pool())
    .await?;
    assert_eq!(
        transitions,
        vec![
            (None, "idle".to_owned()),
            (Some("idle".to_owned()), "requires_action".to_owned()),
        ]
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn simultaneous_first_calls_on_a_new_channel_all_record_on_one_row() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_race");
    let base = Utc::now().trunc_subsecs(6);
    let mut tasks = Vec::new();
    for n in 0..8 {
        let handle = handle.clone();
        let cse_id = cse_id.clone();
        tasks.push(tokio::spawn(async move {
            handle
                .record_remote_control_call(RemoteControlCall {
                    created_at: at(base, n),
                    ..call_at(&cse_id, "POST", EVENTS)
                })
                .await
        }));
    }
    for task in tasks {
        task.await??;
    }
    let channels: i64 =
        sqlx::query_scalar("SELECT count(*) FROM remote_control.channels WHERE cse_id = $1")
            .bind(&cse_id)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(channels, 1);
    assert_eq!(call_count(&handle, &cse_id).await?, 8);

    handle.pool().close().await;
    Ok(())
}
