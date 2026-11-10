pub(crate) const COUNTER_REQUESTS_TOTAL: &str = "proxy_requests_total";

pub(crate) const HISTOGRAM_UPSTREAM_LATENCY: &str = "proxy_upstream_latency_seconds";

pub(crate) const GAUGE_STORE_SIZE: &str = "proxy_store_size";

pub(crate) const GAUGE_STORE_BYTES: &str = "proxy_store_bytes";

pub(crate) const GAUGE_STORE_BYTES_BUDGET: &str = "proxy_store_bytes_budget";

pub(crate) const COUNTER_DIAGNOSTICS_TOTAL: &str = "proxy_diagnostics_total";

pub(crate) const COUNTER_STORE_EVICTIONS_TOTAL: &str = "proxy_store_evictions_total";

pub(crate) const COUNTER_COMPOSITIONS_TOTAL: &str = "claude_proxy_compositions_total";

pub(crate) const COUNTER_COMPOSITIONS_CLASS_TOTAL: &str = "claude_proxy_compositions_class_total";

pub(crate) const HISTOGRAM_DB_QUERY_DURATION_MS: &str = "db_query_duration_ms";

pub(crate) const HISTOGRAM_SEARCH_PHASE_DURATION_MS: &str = "search_phase_duration_ms";

pub(crate) const GAUGE_DB_POOL_SIZE: &str = "db_pool_size";

pub(crate) const GAUGE_DB_POOL_IDLE: &str = "db_pool_idle";

pub(crate) const COUNTER_RETENTION_SWEEP_TICKS_TOTAL: &str = "retention_sweep_ticks_total";

pub(crate) const GAUGE_RETENTION_SWEEP_LAST_RUN_UNIX_SECONDS: &str =
    "retention_sweep_last_run_unix_seconds";

pub(crate) const COUNTER_RETENTION_SWEEP_SESSIONS_REMOVED_TOTAL: &str =
    "retention_sweep_sessions_removed_total";

pub(crate) const COUNTER_RETENTION_SWEEP_MESSAGES_REMOVED_TOTAL: &str =
    "retention_sweep_messages_removed_total";

pub(crate) const COUNTER_UNCAPTURED_REQUESTS_TOTAL: &str = "proxy_uncaptured_requests_total";

pub(crate) const UNCAPTURED_REQUEST_REASONS: &[&str] = &[
    "missing_session_header",
    "malformed_session_header",
    "non_utf8_session_header",
    "capture_throttled",
];

pub(crate) const COUNTER_REMOTE_CONTROL_CALLS_TOTAL: &str = "proxy_remote_control_calls_total";

pub(crate) const REMOTE_CONTROL_CALL_OUTCOMES: &[&str] = &["recorded", "not_persisted", "error"];

pub(crate) const COUNTER_REMOTE_CONTROL_STREAM_EVENTS_TOTAL: &str =
    "proxy_remote_control_stream_events_total";

pub(crate) const REMOTE_CONTROL_STREAM_EVENTS: &[&str] = &[
    "opened",
    "closed_eof",
    "closed_client_disconnect",
    "closed_upstream_error",
];

pub(crate) const COUNTER_CAPTURE_THROTTLE_OVERRIDE_TOTAL: &str =
    "proxy_capture_throttle_override_total";

pub(crate) const GAUGE_MEMORY_RSS_PEAK_BYTES: &str = "proxy_memory_rss_peak_bytes";

pub(crate) const GAUGE_MEMORY_RSS_CURRENT_BYTES: &str = "proxy_memory_rss_current_bytes";

pub(crate) const GAUGE_MEMORY_STORE_ENTRIES: &str = "proxy_memory_store_entries";

pub(crate) const GAUGE_MEMORY_STORE_BYTES: &str = "proxy_memory_store_bytes";

pub(crate) const GAUGE_MEMORY_IN_FLIGHT: &str = "proxy_memory_in_flight";

pub(crate) const GAUGE_MEMORY_BROADCAST_RECEIVERS: &str = "proxy_memory_broadcast_receivers";

pub(crate) const GAUGE_MEMORY_BROADCAST_BACKLOG: &str = "proxy_memory_broadcast_backlog";

pub(crate) const COUNTER_MEMORY_BROADCAST_LAGGED_TOTAL: &str =
    "proxy_memory_broadcast_lagged_total";

pub(crate) const GAUGE_MEMORY_FETCH_ALL_ROWS_IN_FLIGHT: &str =
    "proxy_memory_fetch_all_rows_in_flight";

pub(crate) const GAUGE_MEMORY_FASTEMBED_BYTES: &str = "proxy_memory_fastembed_bytes";

pub(crate) const COUNTER_MEMORY_SSE_BUFFER_BYTES_TOTAL: &str =
    "proxy_memory_sse_buffer_bytes_total";

pub(crate) const COUNTER_MEMORY_SSE_BUFFER_CAPPED_TOTAL: &str =
    "proxy_memory_sse_buffer_capped_total";

pub(crate) const COUNTER_MEMORY_REQUEST_BODY_REJECTED_TOTAL: &str =
    "proxy_memory_request_body_rejected_total";

pub(crate) const COUNTER_MEMORY_SHED_TOTAL: &str = "proxy_memory_shed_total";

pub(crate) const HISTOGRAM_STORE_EVENT_SEND_SECONDS: &str = "proxy_memory_store_event_send_seconds";

pub(crate) const COUNTER_STORE_EVENT_BACKPRESSURED_TOTAL: &str =
    "proxy_memory_store_event_backpressured_total";

pub(crate) const COUNTER_MEMORY_TICKER_RUNS_TOTAL: &str = "proxy_memory_ticker_runs_total";

pub(crate) const HISTOGRAM_MEMORY_TICKER_DURATION_SECONDS: &str =
    "proxy_memory_ticker_duration_seconds";

pub(crate) const HISTOGRAM_MEMORY_BANNER_RENDER_SECONDS: &str =
    "proxy_memory_banner_render_seconds";

pub(crate) const MEMORY_STORE_BYTES_FIELDS: &[&str] = &[
    "request_body",
    "response_body",
    "filtered_request_body",
    "tool_calls",
    "tool_results",
    "tool_results_in_body",
    "system_prompt_blocks",
];

pub(crate) const MEMORY_SSE_BUFFER_PATHS: &[&str] = &[];

pub(crate) const MEMORY_REQUEST_BODY_REJECT_REASONS: &[&str] = &["too_large", "deserialize_failed"];

pub(crate) const MEMORY_SHED_PATHS: &[&str] = &[
    "broadcast_lag",
    "response_body",
    "header_capture",
    "store_event_mpsc",
    "store_insert",
];

pub(crate) const MEMORY_SHED_REASONS: &[&str] =
    &["reject_incoming", "evict_existing", "soft_watermark"];

pub(crate) const MEMORY_TICKER_OUTCOMES: &[&str] = &["ok", "error"];
