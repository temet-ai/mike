use std::time::{Duration, Instant};

use tokio::time::{MissedTickBehavior, interval};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::state::AppState;
use crate::store::models::{MemoryTickPayload, ProxyEvent};
use crate::store::views::MemoryBreakdownView;

#[derive(Debug, PartialEq)]
struct TickSnapshot {
    entry_count: u64,
    entries_total_bytes: u64,
    over_byte_budget: bool,
    request_body_bytes: u64,
    response_body_bytes: u64,
    filtered_request_body_bytes: u64,
    tool_calls_bytes: u64,
    tool_results_bytes: u64,
    tool_results_in_body_bytes: u64,
    system_prompt_blocks_bytes: u64,
    session_index_keys: u64,
    composable_session_index_keys: u64,
    session_meta_keys: u64,
    rss_peak_bytes: Option<u64>,
    fastembed_model_bytes: u64,
    in_flight: usize,
    broadcast_receivers: usize,
    broadcast_backlog: usize,
    db_pool_size: Option<u32>,
    db_pool_idle: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct TickerConfig {
    pub interval: Duration,
}

impl TickerConfig {
    pub fn from_secs(secs: u64) -> Self {
        Self {
            interval: Duration::from_secs(secs),
        }
    }
}

pub fn spawn_memory_ticker(
    state: AppState,
    config: TickerConfig,
    tracker: &TaskTracker,
    cancel: CancellationToken,
) {
    tracker.spawn(async move {
        let mut ticker = interval(config.interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        ticker.tick().await;
        let mut last_snapshot: Option<TickSnapshot> = None;
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => {
                    tracing::debug!(
                        target: "mike::memory_ticker",
                        "memory ticker cancelled"
                    );
                    break;
                }
                _ = ticker.tick() => {
                    last_snapshot = Some(emit_one_tick(&state, last_snapshot.as_ref()).await);
                }
            }
        }
    });
}

#[cfg_attr(not(test), allow(clippy::needless_pass_by_ref_mut))]
async fn emit_one_tick(state: &AppState, last: Option<&TickSnapshot>) -> TickSnapshot {
    let started = Instant::now();

    let breakdown = {
        let store = state.store();
        let guard = store.read().await;
        guard.memory_breakdown(0)
    };

    let rss_peak_bytes = crate::diagnostics::rss::read_max_rss();
    let fastembed_model_bytes = state.embedding_service().attributed_rss_bytes();
    let in_flight = state.in_flight_len();
    let broadcast_receivers = state.event_tx.receiver_count();
    let broadcast_backlog = state.event_tx.len();
    let pool_dims = state.persist().map(|h| {
        let pool = h.pool();
        (pool.size(), pool.num_idle())
    });

    let elapsed = started.elapsed();

    let snapshot = TickSnapshot {
        entry_count: breakdown.entry_count,
        entries_total_bytes: breakdown.entries_total_bytes,
        over_byte_budget: breakdown.over_byte_budget,
        request_body_bytes: breakdown.request_body_bytes,
        response_body_bytes: breakdown.response_body_bytes,
        filtered_request_body_bytes: breakdown.filtered_request_body_bytes,
        tool_calls_bytes: breakdown.tool_calls_bytes,
        tool_results_bytes: breakdown.tool_results_bytes,
        tool_results_in_body_bytes: breakdown.tool_results_in_body_bytes,
        system_prompt_blocks_bytes: breakdown.system_prompt_blocks_bytes,
        session_index_keys: breakdown.session_index_keys,
        composable_session_index_keys: breakdown.composable_session_index_keys,
        session_meta_keys: breakdown.session_meta_keys,
        rss_peak_bytes,
        fastembed_model_bytes,
        in_flight,
        broadcast_receivers,
        broadcast_backlog,
        db_pool_size: pool_dims.map(|(size, _)| size),
        db_pool_idle: pool_dims.map(|(_, idle)| idle),
    };

    broadcast_tick_frame(state, &snapshot, &breakdown, elapsed);
    log_tick_if_changed(&snapshot, &breakdown, last, elapsed);
    record_tick_metrics(&snapshot, elapsed);

    snapshot
}

fn broadcast_tick_frame(
    state: &AppState,
    snapshot: &TickSnapshot,
    breakdown: &MemoryBreakdownView,
    elapsed: Duration,
) {
    let _ = state
        .event_tx
        .send(ProxyEvent::MemoryTick(MemoryTickPayload {
            entry_count: snapshot.entry_count,
            request_body_bytes: snapshot.request_body_bytes,
            response_body_bytes: snapshot.response_body_bytes,
            filtered_request_body_bytes: snapshot.filtered_request_body_bytes,
            tool_calls_bytes: snapshot.tool_calls_bytes,
            tool_results_bytes: snapshot.tool_results_bytes,
            tool_results_in_body_bytes: snapshot.tool_results_in_body_bytes,
            system_prompt_blocks_bytes: snapshot.system_prompt_blocks_bytes,
            entries_total_bytes: snapshot.entries_total_bytes,
            session_index_keys: snapshot.session_index_keys,
            composable_session_index_keys: snapshot.composable_session_index_keys,
            session_meta_keys: snapshot.session_meta_keys,
            in_flight_count: snapshot.in_flight as u64,
            process_rss_bytes: snapshot.rss_peak_bytes,
            fastembed_model_bytes: snapshot.fastembed_model_bytes,
            max_entries: breakdown.max_entries,
            max_store_bytes: breakdown.max_store_bytes,
            over_byte_budget: snapshot.over_byte_budget,
            broadcast_receivers: snapshot.broadcast_receivers,
            broadcast_backlog: snapshot.broadcast_backlog,
            db_pool_size: snapshot.db_pool_size,
            db_pool_idle: snapshot.db_pool_idle,
            tick_duration_ms: elapsed.as_millis() as u64,
        }));
}

fn log_tick_if_changed(
    snapshot: &TickSnapshot,
    breakdown: &MemoryBreakdownView,
    last: Option<&TickSnapshot>,
    elapsed: Duration,
) {
    if last == Some(snapshot) {
        return;
    }
    tracing::debug!(
        target: "mike::memory_ticker",
        event = "memory.tick",
        store_entry_count = snapshot.entry_count,
        store_entries_total_bytes = snapshot.entries_total_bytes,
        store_max_entries = breakdown.max_entries,
        store_max_bytes = breakdown.max_store_bytes,
        store_over_byte_budget = snapshot.over_byte_budget,
        store_request_body_bytes = snapshot.request_body_bytes,
        store_response_body_bytes = snapshot.response_body_bytes,
        store_filtered_request_body_bytes = snapshot.filtered_request_body_bytes,
        store_tool_calls_bytes = snapshot.tool_calls_bytes,
        store_tool_results_bytes = snapshot.tool_results_bytes,
        store_tool_results_in_body_bytes = snapshot.tool_results_in_body_bytes,
        store_system_prompt_blocks_bytes = snapshot.system_prompt_blocks_bytes,
        store_session_index_keys = snapshot.session_index_keys,
        store_composable_session_index_keys = snapshot.composable_session_index_keys,
        store_session_meta_keys = snapshot.session_meta_keys,
        rss_peak_bytes = snapshot.rss_peak_bytes,
        fastembed_model_bytes = snapshot.fastembed_model_bytes,
        in_flight = snapshot.in_flight,
        broadcast_receivers = snapshot.broadcast_receivers,
        broadcast_backlog = snapshot.broadcast_backlog,
        db_pool_size = snapshot.db_pool_size,
        db_pool_idle = snapshot.db_pool_idle,
        tick_duration_ms = elapsed.as_millis() as u64,
        "memory.tick",
    );
}

fn record_tick_metrics(snapshot: &TickSnapshot, elapsed: Duration) {
    metrics::gauge!(crate::metrics::GAUGE_MEMORY_STORE_ENTRIES).set(snapshot.entry_count as f64);
    if let Some(rss) = snapshot.rss_peak_bytes {
        metrics::gauge!(crate::metrics::GAUGE_MEMORY_RSS_PEAK_BYTES).set(rss as f64);
    }
    metrics::gauge!(crate::metrics::GAUGE_MEMORY_FASTEMBED_BYTES)
        .set(snapshot.fastembed_model_bytes as f64);
    metrics::gauge!(crate::metrics::GAUGE_MEMORY_IN_FLIGHT).set(snapshot.in_flight as f64);
    metrics::gauge!(crate::metrics::GAUGE_MEMORY_BROADCAST_RECEIVERS)
        .set(snapshot.broadcast_receivers as f64);
    metrics::gauge!(crate::metrics::GAUGE_MEMORY_BROADCAST_BACKLOG)
        .set(snapshot.broadcast_backlog as f64);

    for (field, bytes) in [
        ("request_body", snapshot.request_body_bytes),
        ("response_body", snapshot.response_body_bytes),
        (
            "filtered_request_body",
            snapshot.filtered_request_body_bytes,
        ),
        ("tool_calls", snapshot.tool_calls_bytes),
        ("tool_results", snapshot.tool_results_bytes),
        ("tool_results_in_body", snapshot.tool_results_in_body_bytes),
        ("system_prompt_blocks", snapshot.system_prompt_blocks_bytes),
    ] {
        metrics::gauge!(crate::metrics::GAUGE_MEMORY_STORE_BYTES, "field" => field)
            .set(bytes as f64);
    }

    metrics::counter!(
        crate::metrics::COUNTER_MEMORY_TICKER_RUNS_TOTAL,
        "outcome" => "ok",
    )
    .increment(1);
    metrics::histogram!(crate::metrics::HISTOGRAM_MEMORY_TICKER_DURATION_SECONDS)
        .record(elapsed.as_secs_f64());
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::config::ProxyConfig;

    #[tokio::test]
    async fn emit_one_tick_runs_without_persistence() -> anyhow::Result<()> {
        let config = ProxyConfig::for_tests_in_memory();
        let state = AppState::new(config).await?;

        let started = Instant::now();
        emit_one_tick(&state, None).await;
        let elapsed = started.elapsed();

        assert!(
            elapsed.as_millis() < 50,
            "tick body must complete in <50ms, took {}ms",
            elapsed.as_millis()
        );
        Ok(())
    }

    #[tokio::test]
    async fn emit_one_tick_suppresses_duplicate() -> anyhow::Result<()> {
        let config = ProxyConfig::for_tests_in_memory();
        let state = AppState::new(config).await?;

        let first = emit_one_tick(&state, None).await;
        emit_one_tick(&state, Some(&first)).await;
        Ok(())
    }

    #[tokio::test]
    async fn emit_one_tick_broadcasts_memory_tick_frame() -> anyhow::Result<()> {
        let config = ProxyConfig::for_tests_in_memory();
        let state = AppState::new(config).await?;

        let mut rx = state.event_tx.subscribe();
        emit_one_tick(&state, None).await;

        let event = rx.try_recv()?;
        let json = serde_json::to_value(&event)?;

        assert_eq!(
            json.get("type").and_then(serde_json::Value::as_str),
            Some("MemoryTick"),
            "frame must carry the MemoryTick discriminator"
        );

        for key in [
            "entry_count",
            "request_body_bytes",
            "response_body_bytes",
            "filtered_request_body_bytes",
            "tool_calls_bytes",
            "tool_results_bytes",
            "tool_results_in_body_bytes",
            "system_prompt_blocks_bytes",
            "entries_total_bytes",
            "session_index_keys",
            "composable_session_index_keys",
            "session_meta_keys",
            "in_flight_count",
            "fastembed_model_bytes",
            "max_entries",
            "max_store_bytes",
            "over_byte_budget",
            "broadcast_receivers",
            "broadcast_backlog",
            "tick_duration_ms",
        ] {
            assert!(
                json.get(key).is_some(),
                "MemoryTick frame missing field `{key}`: {json}"
            );
        }
        for nullable in ["process_rss_bytes", "db_pool_size", "db_pool_idle"] {
            assert!(
                json.as_object().is_some_and(|o| o.contains_key(nullable)),
                "MemoryTick frame missing nullable field `{nullable}`: {json}"
            );
        }
        assert!(
            json["over_byte_budget"].is_boolean(),
            "over_byte_budget must serialize as a JSON bool"
        );
        assert!(
            json.get("top_entries").is_none(),
            "MemoryTick must NOT carry top_entries (lives on /api/diagnostics/memory)"
        );
        Ok(())
    }

    #[test]
    fn ticker_config_from_secs_round_trips() {
        let cfg = TickerConfig::from_secs(5);
        assert_eq!(cfg.interval, Duration::from_secs(5));
    }
}
