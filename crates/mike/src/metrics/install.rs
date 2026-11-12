use std::sync::{Mutex, OnceLock};

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

use super::names::*;

pub(crate) fn install_recorder() -> anyhow::Result<PrometheusHandle> {
    static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();
    static INSTALL_LOCK: Mutex<()> = Mutex::new(());

    if let Some(existing) = HANDLE.get() {
        return Ok(existing.clone());
    }

    let _guard = INSTALL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = HANDLE.get() {
        return Ok(existing.clone());
    }

    let handle = PrometheusBuilder::new()
        .install_recorder()
        .map_err(|e| anyhow::anyhow!("failed to install Prometheus recorder: {e}"))?;

    describe_core_metrics();
    describe_retention_metrics();
    describe_uncaptured_requests_metric();
    describe_capture_throttle_metric();
    describe_memory_metrics();

    prime_memory_series();

    let _ = HANDLE.set(handle.clone());
    Ok(handle)
}

fn describe_core_metrics() {
    metrics::describe_counter!(
        COUNTER_REQUESTS_TOTAL,
        "Total proxy responses grouped by status-code bucket (2xx/4xx/5xx/other)"
    );
    metrics::describe_histogram!(
        HISTOGRAM_UPSTREAM_LATENCY,
        metrics::Unit::Seconds,
        "Upstream round-trip latency per proxied request"
    );
    metrics::describe_gauge!(
        GAUGE_STORE_SIZE,
        "Current number of entries in the in-memory request store"
    );
    metrics::describe_gauge!(
        GAUGE_STORE_BYTES,
        metrics::Unit::Bytes,
        "Aggregate per-entry bytes across the in-memory request store"
    );
    metrics::describe_gauge!(
        GAUGE_STORE_BYTES_BUDGET,
        metrics::Unit::Bytes,
        "Configured byte budget (max_store_bytes) for the in-memory request store; 0 means disabled"
    );
    metrics::describe_counter!(
        COUNTER_DIAGNOSTICS_TOTAL,
        "Total analyzer findings attached to requests, by category and analyzer"
    );
    metrics::describe_counter!(
        COUNTER_STORE_EVICTIONS_TOTAL,
        "Total store evictions by reason (capacity | capacity_bytes | age)"
    );
    metrics::describe_counter!(
        COUNTER_COMPOSITIONS_TOTAL,
        "Total composer-pipeline outcomes (passthrough | templated | unknown_shape | replace_unsupported)"
    );
    metrics::describe_counter!(
        COUNTER_COMPOSITIONS_CLASS_TOTAL,
        "Per-class composer hits (templated outcome only); labels carry prompt_sha256 + label"
    );
    metrics::describe_histogram!(
        HISTOGRAM_DB_QUERY_DURATION_MS,
        metrics::Unit::Milliseconds,
        "Latency of persistence-layer database calls; labels: method + outcome (ok|err for a completed call; dropped_before_completion for a wait abandoned before its result arrived, timed to the drop, whose database outcome is unknown and is not proof of rollback)"
    );
    metrics::describe_histogram!(
        HISTOGRAM_SEARCH_PHASE_DURATION_MS,
        metrics::Unit::Milliseconds,
        "Time spent in one phase of a knowledge search or similar-turn recall that is not a database call; label `phase` names it (knowledge_search_handler, recall_similar_turns_handler, rerank_request)"
    );
    metrics::describe_gauge!(
        GAUGE_DB_POOL_SIZE,
        "Total connections currently held by the persistence PgPool (idle + in-use)"
    );
    metrics::describe_gauge!(
        GAUGE_DB_POOL_IDLE,
        "Idle connections currently parked in the persistence PgPool"
    );
}

fn describe_retention_metrics() {
    metrics::describe_counter!(
        COUNTER_RETENTION_SWEEP_TICKS_TOTAL,
        "Retention-sweep ticks completed, incremented every tick regardless of whether anything was removed — the worker-liveness signal"
    );
    metrics::counter!(COUNTER_RETENTION_SWEEP_TICKS_TOTAL).increment(0);
    metrics::describe_gauge!(
        GAUGE_RETENTION_SWEEP_LAST_RUN_UNIX_SECONDS,
        "Unix timestamp (seconds) of the most recently completed retention-sweep tick"
    );
    metrics::describe_counter!(
        COUNTER_RETENTION_SWEEP_SESSIONS_REMOVED_TOTAL,
        "sessions.sessions rows removed by the retention-sweep worker, cumulative"
    );
    metrics::counter!(COUNTER_RETENTION_SWEEP_SESSIONS_REMOVED_TOTAL).increment(0);
    metrics::describe_counter!(
        COUNTER_RETENTION_SWEEP_MESSAGES_REMOVED_TOTAL,
        "Orphaned messages.messages rows removed by the retention-sweep worker, cumulative"
    );
    metrics::counter!(COUNTER_RETENTION_SWEEP_MESSAGES_REMOVED_TOTAL).increment(0);
}

fn describe_uncaptured_requests_metric() {
    metrics::describe_counter!(
        COUNTER_UNCAPTURED_REQUESTS_TOTAL,
        "Requests forwarded upstream but excluded from inspection capture; label `reason` ∈ {capture_throttled}"
    );
    for reason in UNCAPTURED_REQUEST_REASONS {
        metrics::counter!(COUNTER_UNCAPTURED_REQUESTS_TOTAL, "reason" => *reason).increment(0);
    }
    metrics::describe_counter!(
        COUNTER_SESSIONLESS_REQUESTS_TOTAL,
        "Requests with no canonical session id, forwarded and kept in requests.sessionless_requests; label `reason` ∈ {missing_session_header, malformed_session_header, non_utf8_session_header}"
    );
    for reason in SESSIONLESS_REQUEST_REASONS {
        metrics::counter!(COUNTER_SESSIONLESS_REQUESTS_TOTAL, "reason" => *reason).increment(0);
    }
}

fn describe_capture_throttle_metric() {
    metrics::describe_counter!(
        COUNTER_CAPTURE_THROTTLE_OVERRIDE_TOTAL,
        "Capture-admission-throttled requests captured anyway because the proxy mutated the body (filter/composer/injection) — the captured row is the only record of what was injected"
    );
    metrics::counter!(COUNTER_CAPTURE_THROTTLE_OVERRIDE_TOTAL).increment(0);
}

fn describe_memory_metrics() {
    metrics::describe_gauge!(
        GAUGE_MEMORY_RSS_PEAK_BYTES,
        metrics::Unit::Bytes,
        "Peak resident-set size of the proxy process (getrusage RU_MAXRSS)"
    );
    metrics::describe_gauge!(
        GAUGE_MEMORY_RSS_CURRENT_BYTES,
        metrics::Unit::Bytes,
        "Current resident-set size of the proxy process; populated when current-RSS readback ships"
    );
    metrics::describe_gauge!(
        GAUGE_MEMORY_STORE_ENTRIES,
        "Number of entries in the in-memory request store (alias of proxy_store_size under the proxy_memory_* axis)"
    );
    metrics::describe_gauge!(
        GAUGE_MEMORY_STORE_BYTES,
        metrics::Unit::Bytes,
        "Aggregate bytes per MemoryBreakdownView field across the in-memory request store; label `field` ∈ MemoryBreakdownView byte fields"
    );
    metrics::describe_gauge!(
        GAUGE_MEMORY_IN_FLIGHT,
        "Requests currently in flight through the proxy (in-flight registry length)"
    );
    metrics::describe_gauge!(
        GAUGE_MEMORY_BROADCAST_RECEIVERS,
        "Live receiver count on the proxy event broadcast channel"
    );
    metrics::describe_gauge!(
        GAUGE_MEMORY_BROADCAST_BACKLOG,
        "Current backlog (broadcast::Sender::len) on the proxy event broadcast channel"
    );
    metrics::describe_counter!(
        COUNTER_MEMORY_BROADCAST_LAGGED_TOTAL,
        "Total RecvError::Lagged outcomes observed on broadcast subscribers"
    );
    metrics::describe_gauge!(
        GAUGE_MEMORY_FETCH_ALL_ROWS_IN_FLIGHT,
        "Rows currently held in flight by the body-rehydrate / fetch_all path"
    );
    metrics::describe_gauge!(
        GAUGE_MEMORY_FASTEMBED_BYTES,
        metrics::Unit::Bytes,
        "RSS bytes attributed to the fastembed ONNX model load (peak_rss_after − peak_rss_before); zero when disabled or not yet loaded"
    );
    metrics::describe_counter!(
        COUNTER_MEMORY_SSE_BUFFER_BYTES_TOTAL,
        metrics::Unit::Bytes,
        "Cumulative bytes appended to SSE accumulators (lifetime)"
    );
    metrics::describe_counter!(
        COUNTER_MEMORY_SSE_BUFFER_CAPPED_TOTAL,
        "Times an SSE accumulator's bytes-cap clipped an append; label `path` identifies the accumulator"
    );
    metrics::describe_counter!(
        COUNTER_MEMORY_REQUEST_BODY_REJECTED_TOTAL,
        "Incoming POST bodies refused at the body-cap check; label `reason` ∈ {too_large, deserialize_failed}"
    );
    metrics::describe_counter!(
        COUNTER_MEMORY_SHED_TOTAL,
        "Memory-cap shed events; labels `path` × `reason` (writer + ShedPath/ShedReason enums land in W1.12)"
    );
    metrics::describe_counter!(
        COUNTER_MEMORY_TICKER_RUNS_TOTAL,
        "Steady-state memory ticker observations; label `outcome` ∈ {ok, error}"
    );
    metrics::describe_histogram!(
        HISTOGRAM_MEMORY_TICKER_DURATION_SECONDS,
        metrics::Unit::Seconds,
        "Wall time per steady-state memory ticker tick"
    );
    metrics::describe_histogram!(
        HISTOGRAM_MEMORY_BANNER_RENDER_SECONDS,
        metrics::Unit::Seconds,
        "Wall time spent rendering the startup banner"
    );
}

fn prime_memory_series() {
    metrics::gauge!(GAUGE_MEMORY_RSS_PEAK_BYTES).set(0.0);
    metrics::gauge!(GAUGE_MEMORY_RSS_CURRENT_BYTES).set(0.0);
    metrics::gauge!(GAUGE_MEMORY_STORE_ENTRIES).set(0.0);
    metrics::gauge!(GAUGE_MEMORY_IN_FLIGHT).set(0.0);
    metrics::gauge!(GAUGE_MEMORY_BROADCAST_RECEIVERS).set(0.0);
    metrics::gauge!(GAUGE_MEMORY_BROADCAST_BACKLOG).set(0.0);
    metrics::gauge!(GAUGE_MEMORY_FETCH_ALL_ROWS_IN_FLIGHT).set(0.0);
    metrics::gauge!(GAUGE_MEMORY_FASTEMBED_BYTES).set(0.0);

    metrics::counter!(COUNTER_MEMORY_BROADCAST_LAGGED_TOTAL).increment(0);
    metrics::counter!(COUNTER_MEMORY_SSE_BUFFER_BYTES_TOTAL).increment(0);

    for field in MEMORY_STORE_BYTES_FIELDS {
        metrics::gauge!(GAUGE_MEMORY_STORE_BYTES, "field" => *field).set(0.0);
    }

    for path in MEMORY_SSE_BUFFER_PATHS {
        metrics::counter!(COUNTER_MEMORY_SSE_BUFFER_CAPPED_TOTAL, "path" => *path).increment(0);
    }
    for reason in MEMORY_REQUEST_BODY_REJECT_REASONS {
        metrics::counter!(COUNTER_MEMORY_REQUEST_BODY_REJECTED_TOTAL, "reason" => *reason)
            .increment(0);
    }
    for outcome in MEMORY_TICKER_OUTCOMES {
        metrics::counter!(COUNTER_MEMORY_TICKER_RUNS_TOTAL, "outcome" => *outcome).increment(0);
    }
    for path in MEMORY_SHED_PATHS {
        for reason in MEMORY_SHED_REASONS {
            metrics::counter!(
                COUNTER_MEMORY_SHED_TOTAL,
                "path" => *path,
                "reason" => *reason,
            )
            .increment(0);
        }
    }

    metrics::histogram!(HISTOGRAM_MEMORY_TICKER_DURATION_SECONDS).record(0.0);
    metrics::histogram!(HISTOGRAM_MEMORY_BANNER_RENDER_SECONDS).record(0.0);
}
