use serde_json::json;
use uuid::Uuid;

use super::RuntimeWriteOutcome;
use super::tests::{database, namespace, observation};
use crate::runtime::RuntimeObservation;
use crate::store::persistence::PersistError;

#[tokio::test]
async fn events_raw_payloads_and_future_metadata_roundtrip_without_requests() -> anyhow::Result<()>
{
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let mut envelope = observation(namespace.id)?.envelope;
    envelope.version = 900;
    envelope
        .extra
        .insert("future".into(), json!({"unknown":"\0"}));
    envelope
        .source
        .extra
        .insert("vendor".into(), json!({"future":true}));
    for raw in [
        br#" {"kind":"future/event", "x":1,"x":2} "#.to_vec(),
        vec![0xff, 0, 0xfe, b'{'],
        br#"{"text":"\u0000"}"#.to_vec(),
        b"null".to_vec(),
    ] {
        envelope.ingestion_id = Uuid::new_v4();
        let event = RuntimeObservation::new(envelope.clone(), raw)?;
        assert_eq!(
            handle.append_runtime_observation(&event).await?,
            RuntimeWriteOutcome::Inserted
        );
        let restored = handle
            .runtime_observation(namespace.id, envelope.ingestion_id)
            .await?;
        assert_eq!(restored, Some(event));
    }
    let requests: i64 = sqlx::query_scalar("SELECT count(*) FROM requests.requests")
        .fetch_one(handle.pool())
        .await?;
    assert_eq!(requests, 0);
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn event_replay_is_idempotent_and_changed_bytes_or_metadata_conflict() -> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let event = observation(namespace.id)?;
    assert_eq!(
        handle.append_runtime_observation(&event).await?,
        RuntimeWriteOutcome::Inserted
    );
    assert_eq!(
        handle.append_runtime_observation(&event).await?,
        RuntimeWriteOutcome::Unchanged
    );
    let mut changed = event.clone();
    changed.raw_payload.push(b' ');
    assert!(matches!(
        handle.append_runtime_observation(&changed).await,
        Err(PersistError::DataInvariant(_))
    ));
    changed = event.clone();
    changed.envelope.source.completeness = "complete".into();
    assert!(matches!(
        handle.append_runtime_observation(&changed).await,
        Err(PersistError::DataInvariant(_))
    ));
    let persisted = handle
        .runtime_observation(namespace.id, event.envelope.ingestion_id)
        .await?;
    assert_eq!(persisted, Some(event));
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn identical_item_ids_remain_distinct_observations_and_namespaces() -> anyhow::Result<()> {
    let Some((db, handle, first_namespace)) = database().await? else {
        return Ok(());
    };
    let first = observation(first_namespace.id)?;
    let mut second = first.clone();
    second.envelope.ingestion_id = Uuid::new_v4();
    second.envelope.source_cursor = Some("next-update".into());
    handle.append_runtime_observation(&first).await?;
    assert_eq!(
        handle.append_runtime_observation(&second).await?,
        RuntimeWriteOutcome::Inserted
    );
    let second_namespace = namespace();
    handle.register_runtime_namespace(&second_namespace).await?;
    let mut scoped = first.clone();
    scoped.envelope.namespace_id = second_namespace.id;
    assert_eq!(
        handle.append_runtime_observation(&scoped).await?,
        RuntimeWriteOutcome::Inserted
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM runtime.events WHERE namespace_id = $1")
            .bind(first_namespace.id)
            .fetch_one(handle.pool())
            .await?;
    assert_eq!(count, 2);
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_identical_event_replays_insert_once() -> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let event = observation(namespace.id)?;
    let (first, second) = tokio::join!(
        handle.append_runtime_observation(&event),
        handle.append_runtime_observation(&event),
    );
    let outcomes = [first?, second?];
    assert!(outcomes.contains(&RuntimeWriteOutcome::Inserted));
    assert!(outcomes.contains(&RuntimeWriteOutcome::Unchanged));
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn observation_missing_namespace_and_invalid_projection_fail() -> anyhow::Result<()> {
    let Some((db, handle, namespace)) = database().await? else {
        return Ok(());
    };
    let missing = observation(Uuid::new_v4())?;
    assert!(matches!(
        handle.append_runtime_observation(&missing).await,
        Err(PersistError::Database(_))
    ));
    let mut invalid = observation(namespace.id)?;
    invalid.parsed_payload = None;
    assert!(matches!(
        handle.append_runtime_observation(&invalid).await,
        Err(PersistError::DataInvariant(_))
    ));
    assert!(
        handle
            .runtime_observation(namespace.id, invalid.envelope.ingestion_id)
            .await?
            .is_none()
    );
    handle.pool().close().await;
    db.cleanup().await?;
    Ok(())
}
