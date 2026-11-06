use std::collections::BTreeMap;

use chrono::Utc;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use super::{RuntimeWriteOutcome, same_or_conflict};
use crate::runtime::{
    ProviderThreadRef, RuntimeEventEnvelope, RuntimeEventSource, RuntimeKind, RuntimeNamespace,
    RuntimeObservation, RuntimeThreadBinding,
};
use crate::store::persistence::pool_classes::PoolClasses;
use crate::store::persistence::{PersistError, PersistHandle, test_support};

pub(super) async fn database()
-> anyhow::Result<Option<(test_support::TestDbHandle, PersistHandle, RuntimeNamespace)>> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(None);
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let namespace = namespace();
    handle.register_runtime_namespace(&namespace).await?;
    Ok(Some((db, handle, namespace)))
}

pub(super) fn namespace() -> RuntimeNamespace {
    RuntimeNamespace {
        id: Uuid::new_v4(),
        runtime: RuntimeKind::Codex,
        durable_key: format!("fixture/{}", Uuid::new_v4()),
    }
}

pub(super) fn observation(namespace_id: Uuid) -> anyhow::Result<RuntimeObservation> {
    let envelope = RuntimeEventEnvelope {
        version: 1,
        ingestion_id: Uuid::new_v4(),
        namespace_id,
        source: RuntimeEventSource {
            kind: "fixture".into(),
            source_version: None,
            adapter: "offline".into(),
            adapter_version: "1".into(),
            runtime_version: None,
            fidelity: "original_event_bytes".into(),
            completeness: "unknown".into(),
            extra: BTreeMap::new(),
        },
        received_at: Utc::now(),
        provider_timestamp: None,
        provider_session_id: Some("shared-session".into()),
        provider_thread_id: Some("thread:Case".into()),
        provider_turn_id: Some("turn:non-uuid".into()),
        provider_item_id: Some("item:opaque".into()),
        provider_event_id: None,
        source_cursor: Some("cursor:1".into()),
        extra: BTreeMap::new(),
    };
    Ok(RuntimeObservation::new(
        envelope,
        b"{\"type\":\"future/event\"}".to_vec(),
    )?)
}

pub(super) fn binding(namespace_id: Uuid, mike_session_id: Uuid) -> RuntimeThreadBinding {
    RuntimeThreadBinding {
        mike_session_id,
        provider: ProviderThreadRef {
            namespace_id,
            thread_id: "thread:Case".into(),
        },
        provider_session_id: Some("shared-session".into()),
        owning_seat_id: None,
        native_parent: None,
        forked_from: None,
    }
}

#[test]
fn replay_identical_content_is_unchanged_and_conflicts_fail() -> anyhow::Result<()> {
    let original = observation(Uuid::new_v4())?;
    assert_eq!(
        same_or_conflict(Some(original.clone()), &original, "event")?,
        RuntimeWriteOutcome::Unchanged
    );
    let mut changed = original.clone();
    changed.envelope.source_cursor = Some("different".into());
    assert!(same_or_conflict(Some(original.clone()), &changed, "event").is_err());
    changed = original.clone();
    changed.raw_payload.push(b' ');
    assert!(same_or_conflict(Some(original), &changed, "event").is_err());
    assert!(same_or_conflict(None, &changed, "event").is_err());
    Ok(())
}

#[tokio::test]
async fn closed_pool_all_runtime_operations_return_database_errors() -> anyhow::Result<()> {
    let pool = PgPoolOptions::new().connect_lazy("postgres://localhost/runtime_offline_fixture")?;
    pool.close().await;
    let handle = PersistHandle::from_pools(PoolClasses::single(pool));
    let namespace = namespace();
    let binding = binding(namespace.id, Uuid::new_v4());
    let observation = observation(namespace.id)?;
    let writes = [
        handle.register_runtime_namespace(&namespace).await,
        handle.bind_runtime_thread(&binding).await,
        handle.append_runtime_observation(&observation).await,
    ];
    assert!(
        writes
            .into_iter()
            .all(|value| matches!(value, Err(PersistError::Database(_))))
    );
    assert!(matches!(
        handle.runtime_namespace(namespace.id).await,
        Err(PersistError::Database(_))
    ));
    assert!(matches!(
        handle.runtime_thread(&binding.provider).await,
        Err(PersistError::Database(_))
    ));
    assert!(matches!(
        handle
            .runtime_thread_by_session(binding.mike_session_id)
            .await,
        Err(PersistError::Database(_))
    ));
    assert!(matches!(
        handle
            .runtime_observation(namespace.id, observation.envelope.ingestion_id)
            .await,
        Err(PersistError::Database(_))
    ));
    Ok(())
}
