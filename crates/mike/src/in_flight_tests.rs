use std::sync::Arc;
use std::time::Instant;

use tokio::sync::broadcast;

use super::{InFlightRecord, InFlightRegistry};
use crate::store::models::RequestId;

fn record(session_id: &str) -> InFlightRecord {
    let (tx, _rx) = broadcast::channel(8);
    InFlightRecord {
        session_id: session_id.to_string(),
        started_at: Instant::now(),
        event_tx: tx,
    }
}

#[test]
fn register_then_get_returns_the_inserted_record() -> anyhow::Result<()> {
    let registry = InFlightRegistry::new();
    let id = RequestId::from_raw(101);
    let record = record("session-a");
    registry
        .register(id, record.clone())
        .map_err(|e| anyhow::anyhow!("unbounded register must succeed: {e}"))?;

    let fetched = registry
        .get(id)
        .ok_or_else(|| anyhow::anyhow!("id must be present"))?;
    assert_eq!(fetched.session_id, "session-a");
    assert!(fetched.event_tx.same_channel(&record.event_tx));
    assert!(fetched.elapsed() >= std::time::Duration::ZERO);
    Ok(())
}

#[test]
fn remove_drops_the_entry_and_subsequent_get_returns_none() -> anyhow::Result<()> {
    let registry = InFlightRegistry::new();
    let id = RequestId::from_raw(202);
    registry
        .register(id, record("session-b"))
        .map_err(|e| anyhow::anyhow!("register: {e}"))?;
    assert_eq!(registry.len(), 1);
    registry.remove(id);
    assert_eq!(registry.len(), 0);
    assert!(registry.get(id).is_none());
    Ok(())
}

#[test]
fn remove_of_unknown_id_is_a_silent_noop() {
    let registry = InFlightRegistry::new();
    registry.remove(RequestId::from_raw(303));
    assert_eq!(registry.len(), 0);
}

#[test]
fn register_overwrites_an_existing_record_for_the_same_id() -> anyhow::Result<()> {
    let registry = InFlightRegistry::new();
    let id = RequestId::from_raw(404);
    registry
        .register(id, record("session-c"))
        .map_err(|e| anyhow::anyhow!("register first: {e}"))?;
    registry
        .register(id, record("session-c-second"))
        .map_err(|e| anyhow::anyhow!("register second: {e}"))?;
    assert_eq!(registry.len(), 1);
    let fetched = registry.get(id).ok_or_else(|| anyhow::anyhow!("present"))?;
    assert_eq!(fetched.session_id, "session-c-second");
    Ok(())
}

#[test]
fn register_returns_capacity_error_when_full() -> anyhow::Result<()> {
    let registry = InFlightRegistry::with_capacity(2);
    registry
        .register(RequestId::from_raw(1), record("s1"))
        .map_err(|e| anyhow::anyhow!("first must succeed: {e}"))?;
    registry
        .register(RequestId::from_raw(2), record("s2"))
        .map_err(|e| anyhow::anyhow!("second must succeed: {e}"))?;
    let err = match registry.register(RequestId::from_raw(3), record("s3")) {
        Ok(()) => anyhow::bail!("third register must fail with CapacityError"),
        Err(e) => e,
    };
    assert_eq!(err.current, 2);
    assert_eq!(err.max, 2);
    Ok(())
}

#[test]
fn snapshot_ids_returns_all_keys_in_arbitrary_order() -> anyhow::Result<()> {
    let registry = InFlightRegistry::new();
    registry
        .register(RequestId::from_raw(11), record("s1"))
        .map_err(|e| anyhow::anyhow!("register 11: {e}"))?;
    registry
        .register(RequestId::from_raw(22), record("s2"))
        .map_err(|e| anyhow::anyhow!("register 22: {e}"))?;
    let mut snap: Vec<u64> = registry
        .snapshot_ids()
        .into_iter()
        .map(|i| i.as_u64())
        .collect();
    snap.sort_unstable();
    assert_eq!(snap, vec![11, 22]);
    Ok(())
}

#[test]
fn busy_session_ids_empty_registry_is_empty_and_is_session_busy_false() {
    let registry = InFlightRegistry::new();
    assert!(registry.busy_session_ids().is_empty());
    assert!(!registry.is_session_busy("session-x"));
}

#[test]
fn busy_session_ids_dedups_multiple_requests_for_one_session() -> anyhow::Result<()> {
    let registry = InFlightRegistry::new();
    registry
        .register(RequestId::from_raw(1), record("session-a"))
        .map_err(|e| anyhow::anyhow!("register first: {e}"))?;
    registry
        .register(RequestId::from_raw(2), record("session-a"))
        .map_err(|e| anyhow::anyhow!("register second: {e}"))?;

    let busy = registry.busy_session_ids();
    assert_eq!(busy.len(), 1);
    assert!(busy.contains("session-a"));
    assert!(registry.is_session_busy("session-a"));
    assert!(!registry.is_session_busy("session-b"));
    Ok(())
}

#[test]
fn is_session_busy_flips_to_false_after_last_request_removed() -> anyhow::Result<()> {
    let registry = InFlightRegistry::new();
    let id1 = RequestId::from_raw(10);
    let id2 = RequestId::from_raw(11);
    registry
        .register(id1, record("session-a"))
        .map_err(|e| anyhow::anyhow!("register id1: {e}"))?;
    registry
        .register(id2, record("session-a"))
        .map_err(|e| anyhow::anyhow!("register id2: {e}"))?;

    registry.remove(id1);
    assert!(registry.is_session_busy("session-a"));
    assert!(registry.busy_session_ids().contains("session-a"));

    registry.remove(id2);
    assert!(!registry.is_session_busy("session-a"));
    assert!(registry.busy_session_ids().is_empty());
    Ok(())
}

#[test]
fn busy_session_ids_collects_multiple_distinct_sessions() -> anyhow::Result<()> {
    let registry = InFlightRegistry::new();
    registry
        .register(RequestId::from_raw(1), record("session-a"))
        .map_err(|e| anyhow::anyhow!("register a: {e}"))?;
    registry
        .register(RequestId::from_raw(2), record("session-b"))
        .map_err(|e| anyhow::anyhow!("register b: {e}"))?;
    let busy = registry.busy_session_ids();
    assert_eq!(busy.len(), 2);
    assert!(busy.contains("session-a"));
    assert!(busy.contains("session-b"));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn len_is_correct_under_concurrent_registrations() -> anyhow::Result<()> {
    let registry = Arc::new(InFlightRegistry::new());
    let mut tasks = Vec::new();
    for i in 0..200u64 {
        let registry = registry.clone();
        tasks.push(tokio::spawn(async move {
            let id = RequestId::from_raw(1_000_000 + i);
            let _ = registry.register(id, record(&format!("session-{i}")));
        }));
    }
    for handle in tasks {
        handle.await?;
    }
    assert_eq!(registry.len(), 200);

    let mut tasks = Vec::new();
    for i in 0..200u64 {
        let registry = registry.clone();
        tasks.push(tokio::spawn(async move {
            let id = RequestId::from_raw(1_000_000 + i);
            registry.remove(id);
        }));
    }
    for handle in tasks {
        handle.await?;
    }
    assert_eq!(registry.len(), 0);
    for i in 0..200u64 {
        assert!(registry.get(RequestId::from_raw(1_000_000 + i)).is_none());
    }
    Ok(())
}
