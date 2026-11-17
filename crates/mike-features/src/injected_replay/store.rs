use std::sync::Arc;
use std::time::Duration;

use crate::base_state::BaseState;
use crate::store::persistence::proxy_repo::ProxyPersist as _;

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
        match persist.proxy().live_injected_replay_records(machine).await {
            Ok(rows) => {
                let loaded = rows.len();
                for row in rows {
                    if let Some(record) =
                        InjectedBlocks::restored(loaded_identity(row.message_identity), row.tail)
                    {
                        registry.restore(&row.lane, record);
                    }
                }
                tracing::info!(loaded, "injected replay: restored the last hour's records");
            }
            Err(err) => {
                tracing::warn!(error = %err, "injected replay: could not load stored records");
            }
        }
        loop {
            if let Err(err) = persist
                .proxy()
                .expire_injected_replay_records(machine)
                .await
            {
                tracing::warn!(error = %err, "injected replay: could not expire old records");
            }
            tokio::select! {
                () = shutdown.cancelled() => return,
                () = tokio::time::sleep(EXPIRE_EVERY) => {}
            }
        }
    });
}

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
