use chrono::{DateTime, Duration, SubsecRound, Utc};
use serde_json::Value;

use mike::store::persistence::PersistHandle;
use mike::store::persistence::remote_control::{
    PATH_DELIVERY, PATH_HEARTBEAT, PATH_STREAM, RemoteControlCall, RemoteControlPayload,
    RemoteControlSignals,
};

use crate::store::remote_control::support::{call_at, digest, shared_handle, unique};

const KEPT_BODY: &[u8] = br#"{"type":"permission_request","tool":"Bash"}"#;

struct BurstInputs<'a> {
    base: DateTime<Utc>,
    payloads: &'a [RemoteControlPayload<'a>],
    preserved: &'a [&'a str],
    summary: &'a Value,
    session: &'a str,
}

fn at(base: DateTime<Utc>, seconds: i64) -> DateTime<Utc> {
    base + Duration::seconds(seconds)
}

fn channel_open_burst<'a>(cse_id: &'a str, inputs: &BurstInputs<'a>) -> Vec<RemoteControlCall<'a>> {
    let base = inputs.base;
    let status = |seconds, status| RemoteControlCall {
        created_at: at(base, seconds),
        signals: RemoteControlSignals {
            worker_status: Some(status),
            client_session_id: Some("cs_burst"),
            worker_epoch: Some(seconds),
            local_session_uuid: Some(inputs.session),
            local_cwd: Some("/work/burst"),
            client_version: Some("2.1.300"),
            ..RemoteControlSignals::default()
        },
        ..call_at(cse_id, "PUT", "/worker")
    };
    vec![
        RemoteControlCall {
            created_at: at(base, 0),
            ..call_at(cse_id, "POST", "/bridge")
        },
        status(1, "idle"),
        RemoteControlCall {
            created_at: at(base, 2),
            signals: RemoteControlSignals {
                idle_seconds: Some(3),
                current_interval_seconds: Some(20),
                ..RemoteControlSignals::default()
            },
            ..call_at(cse_id, "POST", PATH_HEARTBEAT)
        },
        RemoteControlCall {
            created_at: at(base, 3),
            payloads: inputs.payloads,
            signals: RemoteControlSignals {
                preserved_event_ids: Some(inputs.preserved),
                is_compaction: Some(false),
                post_turn_summary: Some(inputs.summary),
                ..RemoteControlSignals::default()
            },
            ..call_at(cse_id, "POST", "/worker/events")
        },
        RemoteControlCall {
            created_at: at(base, 4),
            ..call_at(cse_id, "POST", PATH_DELIVERY)
        },
        RemoteControlCall {
            created_at: at(base, 5),
            ..call_at(cse_id, "POST", "/client/presence")
        },
        RemoteControlCall {
            created_at: at(base, 6),
            completed_at: None,
            ..call_at(cse_id, "GET", PATH_STREAM)
        },
        status(7, "requires_action"),
        status(8, "idle"),
    ]
    .into_iter()
    .map(|call| RemoteControlCall {
        completed_at: call.completed_at.map(|_| call.created_at),
        ..call
    })
    .collect()
}

async fn channel_state(handle: &PersistHandle, cse_id: &str) -> anyhow::Result<Value> {
    Ok(sqlx::query_scalar(
        "SELECT jsonb_build_object(
             'channel', to_jsonb(ch) - 'id' - 'cse_id',
             'calls', (SELECT jsonb_agg(to_jsonb(c) - 'id' - 'channel_id' ORDER BY c.created_at)
                         FROM remote_control.calls c WHERE c.channel_id = ch.id),
             'payloads', (SELECT jsonb_agg(jsonb_build_object(
                                 'path', c.path, 'ordinal', p.ordinal, 'type', p.payload_type,
                                 'size', p.body_size) ORDER BY c.created_at, p.ordinal)
                            FROM remote_control.call_payloads p
                            JOIN remote_control.calls c ON c.id = p.call_id
                           WHERE c.channel_id = ch.id),
             'transitions', (SELECT jsonb_agg(jsonb_build_object(
                                    'from', t.from_status, 'to', t.to_status,
                                    'observed_at', t.observed_at, 'call_at', c.created_at)
                                    ORDER BY t.observed_at)
                               FROM remote_control.worker_status_transitions t
                               LEFT JOIN remote_control.calls c ON c.id = t.call_id
                              WHERE t.channel_id = ch.id))
           FROM remote_control.channels ch WHERE ch.cse_id = $1",
    )
    .bind(cse_id)
    .fetch_one(handle.pool())
    .await?)
}

#[tokio::test]
async fn record_remote_control_calls_with_a_channel_open_burst_matches_sequential_recording()
-> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let payloads = [RemoteControlPayload {
        ordinal: 0,
        payload_type: "permission_request",
        body: Some(KEPT_BODY),
        body_sha256: digest(KEPT_BODY),
        body_size: i32::try_from(KEPT_BODY.len())?,
    }];
    let preserved = ["evt_one", "evt_two"];
    let summary = serde_json::json!({"title": "burst"});
    let session = uuid::Uuid::new_v4().to_string();
    let inputs = BurstInputs {
        base: Utc::now().trunc_subsecs(6),
        payloads: &payloads,
        preserved: &preserved,
        summary: &summary,
        session: &session,
    };

    let sequential = unique("cse_sequential");
    let one_by_one = channel_open_burst(&sequential, &inputs);
    let mut sequential_ids = Vec::new();
    for call in one_by_one.iter().cloned() {
        sequential_ids.push(handle.record_remote_control_call(call).await?);
    }

    let batched = unique("cse_batched");
    let burst = channel_open_burst(&batched, &inputs);
    let batched_ids = handle.record_remote_control_calls(&burst).await?;

    assert_eq!(batched_ids.len(), burst.len());
    assert!(batched_ids.windows(2).all(|pair| pair[0] < pair[1]));
    let expected = channel_state(&handle, &sequential).await?;
    assert_eq!(channel_state(&handle, &batched).await?, expected);
    assert_eq!(
        expected["transitions"].as_array().map(Vec::len),
        Some(3),
        "the burst moves the status three times: {expected}"
    );

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn record_remote_control_calls_with_two_channels_refuses_the_batch() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let first = unique("cse_mixed_a");
    let second = unique("cse_mixed_b");
    let mixed = [
        call_at(&first, "POST", "/worker/events"),
        call_at(&second, "POST", "/worker/events"),
    ];
    let Err(error) = handle.record_remote_control_calls(&mixed).await else {
        anyhow::bail!("a batch holding two channels was recorded");
    };
    assert!(error.to_string().contains(&second), "{error}");
    let recorded: i64 =
        sqlx::query_scalar("SELECT count(*) FROM remote_control.channels WHERE cse_id = ANY($1)")
            .bind(vec![first.clone(), second.clone()])
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(recorded, 0);

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn record_remote_control_calls_with_no_calls_records_nothing() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    assert!(handle.record_remote_control_calls(&[]).await?.is_empty());
    handle.pool().close().await;
    Ok(())
}
