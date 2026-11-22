use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use crate::base_state::BaseState;
use crate::store::persistence::proxy_repo::ProxyPersist as _;
use crate::store::persistence::{PersistError, PersistHandle};

use super::InjectedBlockRegistry;
use super::splice::InjectedBlocks;

const EXPIRE_EVERY: Duration = Duration::from_secs(600);

fn stored_identity(identity: u64) -> i64 {
    i64::from_be_bytes(identity.to_be_bytes())
}

fn loaded_identity(stored: i64) -> u64 {
    u64::from_be_bytes(stored.to_be_bytes())
}

pub fn persist(state: &BaseState, lane: &str, record: &InjectedBlocks) {
    let Some(persist) = state.persist().cloned() else {
        return;
    };
    let (state, lane, identity, tail) = (
        state.clone(),
        lane.to_owned(),
        stored_identity(record.identity),
        record.tail().to_vec(),
    );
    state.tracker.clone().spawn(async move {
        let Some(machine) = state.resolve_host_machine_uuid().await else {
            return;
        };
        if let Err(err) = persist
            .proxy()
            .insert_injected_replay_record(machine, &lane, identity, &tail)
            .await
        {
            tracing::warn!(lane, error = %err, "injected replay: could not store a record, so a full re-send after a restart misses it");
        }
    });
}

pub async fn restore_stored(
    persist: &PersistHandle,
    machine: Uuid,
    registry: &InjectedBlockRegistry,
) -> Result<usize, PersistError> {
    let rows = persist
        .proxy()
        .live_injected_replay_records(machine)
        .await?;
    let loaded = rows.len();
    for row in rows {
        if let Some(record) =
            InjectedBlocks::restored(loaded_identity(row.message_identity), row.tail)
        {
            registry.restore(&row.lane, record);
        }
    }
    Ok(loaded)
}

pub async fn keep_lanes_in_use(
    persist: &PersistHandle,
    machine: Uuid,
    registry: &InjectedBlockRegistry,
) -> Result<u64, PersistError> {
    let lanes = registry.take_lanes_in_use();
    let kept = persist
        .proxy()
        .keep_injected_replay_records(machine, &lanes)
        .await;
    if kept.is_err() {
        registry.mark_in_use(lanes);
    }
    kept
}

pub async fn expire_unused(
    persist: &PersistHandle,
    machine: Uuid,
    registry: &InjectedBlockRegistry,
) -> Result<usize, PersistError> {
    let expired: Vec<(String, u64)> = persist
        .proxy()
        .expire_injected_replay_records(machine)
        .await?
        .into_iter()
        .map(|row| (row.lane, loaded_identity(row.message_identity)))
        .collect();
    registry.forget(&expired);
    Ok(expired.len())
}

fn warn_keep_failed(err: &PersistError) {
    tracing::warn!(error = %err, "injected replay: could not keep the records of lanes in use, so none expire this round");
}

fn warn_expire_failed(err: &PersistError) {
    tracing::warn!(error = %err, "injected replay: could not expire old records");
}

pub async fn sweep(persist: &PersistHandle, machine: Uuid, registry: &InjectedBlockRegistry) {
    if let Err(err) = keep_lanes_in_use(persist, machine, registry).await {
        return warn_keep_failed(&err);
    }
    if let Err(err) = expire_unused(persist, machine, registry).await {
        warn_expire_failed(&err);
    }
}

pub fn spawn_injected_replay_store(state: &BaseState, registry: Arc<InjectedBlockRegistry>) {
    let state = state.clone();
    let shutdown = state.shutdown();
    state.tracker.clone().spawn(async move {
        let Some(persist) = state.persist().cloned() else {
            return;
        };
        let Some(machine) = state.resolve_host_machine_uuid().await else {
            tracing::warn!(
                "injected replay: no machine identity, so stored records are not loaded"
            );
            return;
        };
        match restore_stored(&persist, machine, &registry).await {
            Ok(loaded) => {
                tracing::info!(loaded, "injected replay: restored the last hour's records");
            }
            Err(err) => {
                tracing::warn!(error = %err, "injected replay: could not load stored records");
            }
        }
        loop {
            sweep(&persist, machine, &registry).await;
            tokio::select! {
                () = shutdown.cancelled() => {
                    if let Err(err) = keep_lanes_in_use(&persist, machine, &registry).await {
                        warn_keep_failed(&err);
                    }
                    return;
                }
                () = tokio::time::sleep(EXPIRE_EVERY) => {}
            }
        }
    });
}

#[cfg(test)]
#[path = "store_db_tests.rs"]
mod store_db_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_round_trips_through_the_stored_column() {
        for identity in [0, 1, u64::MAX, 1 << 63, 0x0123_4567_89ab_cdef] {
            assert_eq!(loaded_identity(stored_identity(identity)), identity);
        }
    }
}
