use crate::state::AppState;
use crate::store::retention::spawn_pruner;

pub(super) struct ObservabilityHandles {
    pruner_handle: tokio::task::JoinHandle<()>,
    gauge_handle: tokio::task::JoinHandle<()>,
    db_pool_gauge_handle: Option<tokio::task::JoinHandle<()>>,
}

pub(super) fn spawn_observability_tasks(state: &AppState) -> ObservabilityHandles {
    let pruner_handle = spawn_pruner(state.store(), state.retention(), state.shutdown());

    let gauge_handle = crate::metrics::spawn_store_gauge_updater(state.store(), state.shutdown());

    let db_pool_gauge_handle = state.spawn_db_pool_gauge_updater();

    crate::observability::ticker::spawn_memory_ticker(
        state.clone(),
        crate::observability::ticker::TickerConfig::from_secs(5),
        &state.tracker(),
        state.shutdown(),
    );

    crate::memory_cap::manager::spawn_cap_manager(
        state.clone(),
        state.config().cap_manager.clone(),
        &state.tracker(),
        state.shutdown(),
    );

    ObservabilityHandles {
        pruner_handle,
        gauge_handle,
        db_pool_gauge_handle,
    }
}

pub(super) async fn wait_for_observability_tasks(handles: ObservabilityHandles) {
    let _ = tokio::time::timeout(std::time::Duration::from_secs(30), handles.pruner_handle).await;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(30), handles.gauge_handle).await;
    if let Some(handle) = handles.db_pool_gauge_handle {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(30), handle).await;
    }
}
