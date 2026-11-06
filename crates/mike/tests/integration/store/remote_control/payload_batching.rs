use mike::store::persistence::remote_control::{
    PATH_HEARTBEAT, RemoteControlCall, RemoteControlPayload, RemoteControlSignals,
};

use crate::store::remote_control::support::{call_at, digest, shared_handle, unique};

const PERMISSION_REQUEST_BODY: &[u8] = br#"{"type":"permission_request","tool":"Bash"}"#;
const TASK_STATUS_BODY: &[u8] = br#"{"type":"task_status","state":"running"}"#;
const STREAM_EVENT_BODY: &[u8] = br#"{"type":"stream_event","delta":"..."}"#;

fn realistic_worker_events_batch() -> anyhow::Result<Vec<RemoteControlPayload<'static>>> {
    let mut payloads: Vec<RemoteControlPayload<'static>> = (0..100)
        .map(|i| RemoteControlPayload {
            ordinal: i,
            payload_type: "stream_event",
            body: None,
            body_sha256: digest(STREAM_EVENT_BODY),
            body_size: i32::try_from(STREAM_EVENT_BODY.len()).unwrap_or(0),
        })
        .collect();
    for (idx, (kind, body)) in [
        ("permission_request", PERMISSION_REQUEST_BODY),
        ("permission_response", PERMISSION_REQUEST_BODY),
        ("task_status", TASK_STATUS_BODY),
    ]
    .into_iter()
    .enumerate()
    {
        let at = [12usize, 47, 88][idx];
        payloads[at] = RemoteControlPayload {
            ordinal: i32::try_from(at)?,
            payload_type: kind,
            body: Some(body),
            body_sha256: digest(body),
            body_size: i32::try_from(body.len())?,
        };
    }
    Ok(payloads)
}

async fn assert_call_payload_rows_kept_only(
    pool: &sqlx::PgPool,
    call_id: i64,
) -> anyhow::Result<()> {
    let rows = sqlx::query!(
        r#"SELECT ordinal, payload_type, body, body_size
             FROM remote_control.call_payloads
            WHERE call_id = $1 ORDER BY ordinal"#,
        call_id
    )
    .fetch_all(pool)
    .await?;
    assert_eq!(
        rows.len(),
        3,
        "a row per kept item, not a row per item: 97 stream_event frames must not become rows"
    );
    assert_eq!(
        rows.iter().map(|r| r.ordinal).collect::<Vec<_>>(),
        vec![12, 47, 88],
        "ordinals survive, so a kept item is still locatable within its batch"
    );
    assert_eq!(rows[0].payload_type, "permission_request");
    assert_eq!(
        &rows[0].body[..],
        PERMISSION_REQUEST_BODY,
        "a kept payload is its exact original bytes"
    );
    Ok(())
}

async fn assert_call_aggregate_counts(pool: &sqlx::PgPool, call_id: i64) -> anyhow::Result<()> {
    let call = sqlx::query!(
        r#"SELECT request_body, request_body_size, payload_count, payload_kept_count,
                  payload_type_counts AS "counts: serde_json::Value"
             FROM remote_control.calls WHERE id = $1"#,
        call_id
    )
    .fetch_one(pool)
    .await?;
    assert_eq!(call.payload_count, Some(100));
    assert_eq!(call.payload_kept_count, Some(3));
    assert_eq!(
        call.request_body, None,
        "the batch body is not stored a second time alongside its payload rows"
    );
    assert_eq!(
        call.request_body_size,
        Some(402_911),
        "but its true size is still recorded, so the batch stays accountable"
    );
    let counts = call.counts.unwrap_or_default();
    assert_eq!(
        counts
            .get("stream_event")
            .and_then(serde_json::Value::as_i64),
        Some(97),
        "the dropped frames survive as a count — the only record they existed"
    );
    assert_eq!(
        counts
            .get("permission_request")
            .and_then(serde_json::Value::as_i64),
        Some(1)
    );
    Ok(())
}

#[tokio::test]
async fn a_batch_writes_rows_only_for_the_payloads_it_kept() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_batch");

    let payloads = realistic_worker_events_batch()?;
    let call_id = handle
        .record_remote_control_call(RemoteControlCall {
            request_body: None,
            request_body_sha256: Some(digest(b"the whole batch")),
            request_body_size: Some(402_911),
            payloads: &payloads,
            ..call_at(&cse_id, "POST", "/worker/events")
        })
        .await?;

    assert_call_payload_rows_kept_only(handle.pool(), call_id).await?;
    assert_call_aggregate_counts(handle.pool(), call_id).await?;

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn a_call_with_no_batch_records_no_payload_accounting() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_nobatch");

    let call_id = handle
        .record_remote_control_call(call_at(&cse_id, "POST", PATH_HEARTBEAT))
        .await?;

    let row = sqlx::query!(
        r#"SELECT payload_count, payload_kept_count,
                  payload_type_counts AS "counts: serde_json::Value"
             FROM remote_control.calls WHERE id = $1"#,
        call_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(row.payload_count, None);
    assert_eq!(row.payload_kept_count, None);
    assert_eq!(row.counts, None);

    let payloads = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM remote_control.call_payloads WHERE call_id = $1"#,
        call_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(payloads, 0);

    handle.pool().close().await;
    Ok(())
}

#[tokio::test]
async fn internal_event_compaction_fields_are_queryable_columns() -> anyhow::Result<()> {
    let Some(handle) = shared_handle().await? else {
        return Ok(());
    };
    let cse_id = unique("cse_internal");

    let preserved = ["evt-1", "evt-2", "evt-3"];
    let body = br#"{"payload":{"type":"compaction"},"is_compaction":true}"#;
    let call_id = handle
        .record_remote_control_call(RemoteControlCall {
            request_body: Some(body),
            request_body_sha256: Some(digest(body)),
            request_body_size: Some(i32::try_from(body.len())?),
            signals: RemoteControlSignals {
                is_compaction: Some(true),
                session_agent_id: Some("agent-7"),
                preserved_event_ids: Some(&preserved),
                ..RemoteControlSignals::default()
            },
            ..call_at(&cse_id, "POST", "/worker/internal-events")
        })
        .await?;

    let row = sqlx::query!(
        r#"SELECT session_agent_id, preserved_event_ids, request_body
             FROM remote_control.calls
            WHERE id = $1 AND is_compaction AND 'evt-2' = ANY(preserved_event_ids)"#,
        call_id
    )
    .fetch_one(handle.pool())
    .await?;
    assert_eq!(row.session_agent_id.as_deref(), Some("agent-7"));
    assert_eq!(
        row.preserved_event_ids.as_deref(),
        Some(
            [
                "evt-1".to_string(),
                "evt-2".to_string(),
                "evt-3".to_string()
            ]
            .as_slice()
        )
    );
    assert_eq!(
        row.request_body.as_deref(),
        Some(body.as_slice()),
        "internal-events duplicates nothing, so its body is kept whole as well"
    );

    handle.pool().close().await;
    Ok(())
}
