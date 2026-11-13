use std::time::Instant;

use tracing::info;

use crate::state::AppState;

pub async fn emit_startup_banner(state: &AppState) {
    let started = Instant::now();
    let config = state.config();

    let process_rss_bytes = crate::diagnostics::rss::read_max_rss();
    let process_pid = std::process::id();
    let process_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(0);

    let memory = state.store().read().await.memory_breakdown(0);

    let pool_dims = state.persist().map(|h| h.pool_connection_stats());

    let row_counts = match state.persist() {
        Some(handle) => Some(handle.startup_row_counts().await.unwrap_or_default()),
        None => None,
    };

    let embedding = state.embedding_service();
    let embedding_enabled = embedding.is_available();
    let embedding_model = embedding.model_name();
    let embedding_rss_bytes = embedding.attributed_rss_bytes();
    let embedding_warm = embedding_rss_bytes > 0;

    let banner_render_ms = started.elapsed().as_millis() as u64;

    let (rows_requests, rows_sessions, rows_tool_uses, rows_tool_results, rows_embeddings) =
        match row_counts {
            Some(counts) => (
                Some(counts.requests),
                Some(counts.sessions),
                Some(counts.tool_uses),
                Some(counts.tool_results),
                Some(counts.chunks),
            ),
            None => (None, None, None, None, None),
        };

    let (pool_size_now, pool_idle_now) = match pool_dims {
        Some((size, idle)) => (Some(size), Some(idle)),
        None => (None, None),
    };

    info!(
        target: "mike::startup_banner",
        event = "startup.banner",
        version = env!("CARGO_PKG_VERSION"),
        pid = process_pid,
        cpus = process_cpus,
        process_rss_bytes = process_rss_bytes,
        proxy_listen_addr = %config.proxy_listen_addr,
        inspect_listen_addr = %config.inspect_listen_addr,
        upstream_url = %config.upstream_url,
        budget_max_request_body_bytes = config.max_request_body_bytes,
        budget_max_store_bytes = config.max_store_bytes,
        budget_max_in_flight = config.max_in_flight,
        store_entry_count = memory.entry_count,
        store_entries_total_bytes = memory.entries_total_bytes,
        store_max_entries = memory.max_entries,
        feature_persist_enabled = state.persist().is_some(),
        feature_composition_enabled = config.composition_enabled,
        feature_embedding_enabled = embedding_enabled,
        embedding_model = embedding_model,
        embedding_warm = embedding_warm,
        embedding_rss_bytes = embedding_rss_bytes,
        db_pool_size_now = pool_size_now,
        db_pool_idle_now = pool_idle_now,
        db_rows_requests = rows_requests,
        db_rows_sessions = rows_sessions,
        db_rows_tool_uses = rows_tool_uses,
        db_rows_tool_results = rows_tool_results,
        db_rows_embeddings = rows_embeddings,
        banner_render_ms,
        "mike startup banner",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProxyConfig;

    #[tokio::test]
    async fn emit_startup_banner_runs_without_persistence() -> anyhow::Result<()> {
        let config = ProxyConfig::for_tests_in_memory();
        let state = AppState::new(config).await?;

        let started = Instant::now();
        emit_startup_banner(&state).await;
        let elapsed = started.elapsed();

        assert!(
            elapsed.as_secs() < 2,
            "banner emission must complete within 2 s, took {elapsed:?}",
        );
        Ok(())
    }

    #[tokio::test]
    async fn emit_startup_banner_reflects_empty_store() -> anyhow::Result<()> {
        let config = ProxyConfig::for_tests_in_memory();
        let state = AppState::new(config).await?;

        let view = state.store().read().await.memory_breakdown(0);
        assert_eq!(view.entry_count, 0);

        emit_startup_banner(&state).await;
        Ok(())
    }
}
