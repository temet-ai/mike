use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::config::{AppState, primary_tcp_unreachable};
use crate::resync::{ResyncEngine, ResyncError, SyncReport};
use crate::store::persistence::ActiveDbTarget;

const WORKER_POOL_MAX_CONNECTIONS: u32 = 4;

const WORKER_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ResyncReportSummary {
    pub tables: Vec<(String, u64)>,
    pub total_rows: u64,
}

impl ResyncReportSummary {
    fn from_report(report: &SyncReport) -> Self {
        Self {
            tables: report.per_table.clone(),
            total_rows: report.total_rows,
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ResyncStatus {
    pub last_run_at: Option<DateTime<Utc>>,
    pub dest_reachable: bool,
    pub last_report: ResyncReportSummary,
    pub last_error: Option<String>,
}

pub type ResyncStatusHandle = Arc<ArcSwap<ResyncStatus>>;

fn db_resync_config_enabled(state: &AppState) -> bool {
    if state.config.db_resync_enabled {
        return true;
    }
    tracing::debug!("resync_worker: db_resync_enabled = false (boot config) — skipping spawn");
    false
}

fn db_resync_toggle_enabled(state: &AppState) -> bool {
    if state.feature_toggle_overlay.load().db_resync {
        return true;
    }
    tracing::debug!("resync_worker: db_resync feature toggle off — skipping spawn");
    false
}

fn resync_urls(state: &AppState) -> Option<(String, String)> {
    let (Some(dest_url), Some(source_url)) = (
        state.config.database_url.clone(),
        state.config.local_database_url.clone(),
    ) else {
        tracing::debug!(
            "resync_worker: database_url and/or local_database_url unset — skipping spawn (resync needs both source and dest)"
        );
        return None;
    };
    Some((source_url, dest_url))
}

fn booted_on_local_fallback(state: &AppState) -> bool {
    let on_fallback = state
        .persist
        .as_ref()
        .is_some_and(|p| p.active_target() == ActiveDbTarget::LocalFallback);
    if on_fallback {
        tracing::debug!(
            "resync_worker: booted on local fallback — skipping this run's drain (next restart against remote drains it)"
        );
    }
    on_fallback
}

pub fn spawn_resync_worker(state: &AppState) {
    if !db_resync_config_enabled(state) {
        return;
    }
    if !db_resync_toggle_enabled(state) {
        return;
    }
    let Some((source_url, dest_url)) = resync_urls(state) else {
        return;
    };
    if booted_on_local_fallback(state) {
        return;
    }

    let status = state.resync_status.clone();

    state.tracker.spawn(async move {
        run(source_url, dest_url, status).await;
    });
}

fn build_worker_pool(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(WORKER_POOL_MAX_CONNECTIONS)
        .min_connections(0)
        .acquire_timeout(WORKER_ACQUIRE_TIMEOUT)
        .connect_lazy(url)
}

fn log_source_pool_error(e: &sqlx::Error) {
    tracing::error!(error = %e, "resync_worker: source pool URL unparseable — drain not running");
}

fn log_dest_pool_error(e: &sqlx::Error) {
    tracing::error!(error = %e, "resync_worker: dest pool URL unparseable — drain not running");
}

fn build_worker_pools(source_url: &str, dest_url: &str) -> Option<(PgPool, PgPool)> {
    match (build_worker_pool(source_url), build_worker_pool(dest_url)) {
        (Ok(source), Ok(dest)) => Some((source, dest)),
        (source, dest) => {
            if let Err(e) = &source {
                log_source_pool_error(e);
            }
            if let Err(e) = &dest {
                log_dest_pool_error(e);
            }
            None
        }
    }
}

async fn finish_drain(
    result: Result<(), ResyncError>,
    source: &PgPool,
    dest: &PgPool,
    status: &ResyncStatusHandle,
) {
    match result {
        Ok(()) => {
            if status.load().dest_reachable {
                maybe_reset_local(source, dest, status).await;
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "resync_worker: startup drain failed");
        }
    }
}

fn log_drain_starting() {
    tracing::info!("resync_worker: starting startup drain");
}

fn log_source_unreachable() {
    tracing::debug!("resync_worker: source (local) unreachable — nothing to drain");
}

async fn run(source_url: String, dest_url: String, status: ResyncStatusHandle) {
    log_drain_starting();

    if primary_tcp_unreachable(&source_url).await {
        log_source_unreachable();
        return;
    }

    let Some((source, dest)) = build_worker_pools(&source_url, &dest_url) else {
        return;
    };

    let result = run_once(&source, &dest, &dest_url, &status).await;
    finish_drain(result, &source, &dest, &status).await;

    source.close().await;
    dest.close().await;
}

fn log_local_reset_ok() {
    tracing::info!("resync_worker: local DB reset — empty and current for the next outage");
}

fn record_reset_failure(status: &ResyncStatusHandle, e: &ResyncError) {
    tracing::error!(error = %e, "resync_worker: local DB reset failed after drain");
    update_status(status, |s| {
        s.last_error = Some(format!("local reset failed: {e}"));
    });
}

async fn reset_local_and_record(source: &PgPool, status: &ResyncStatusHandle) {
    match crate::resync::reset::reset_local(source).await {
        Ok(()) => log_local_reset_ok(),
        Err(e) => record_reset_failure(status, &e),
    }
}

fn log_rows_arrived_during_drain() {
    tracing::debug!(
        "resync_worker: source rows arrived during the drain — skipping local reset this run"
    );
}

fn log_pending_check_failed(e: &ResyncError) {
    tracing::warn!(error = %e, "resync_worker: post-drain pending check failed — skipping local reset");
}

async fn maybe_reset_local(source: &PgPool, dest: &PgPool, status: &ResyncStatusHandle) {
    let engine = ResyncEngine::new(source.clone(), dest.clone());
    match engine.has_pending().await {
        Ok(true) => log_rows_arrived_during_drain(),
        Ok(false) => reset_local_and_record(source, status).await,
        Err(e) => log_pending_check_failed(&e),
    }
}

fn record_dest_unreachable(status: &ResyncStatusHandle, now: DateTime<Utc>) {
    update_status(status, |s| {
        s.last_run_at = Some(now);
        s.dest_reachable = false;
    });
}

fn log_drain_nonempty(report: &SyncReport) {
    tracing::info!(
        total_rows = report.total_rows,
        per_table = ?report.per_table,
        "resync_worker: drained local rows into remote"
    );
}

fn log_drain_empty() {
    tracing::debug!("resync_worker: nothing pending to drain");
}

fn log_drain_report(report: &SyncReport) {
    if report.total_rows > 0 {
        log_drain_nonempty(report);
    } else {
        log_drain_empty();
    }
}

fn record_drain_success(status: &ResyncStatusHandle, now: DateTime<Utc>, report: &SyncReport) {
    let summary = ResyncReportSummary::from_report(report);
    update_status(status, |s| {
        s.last_run_at = Some(now);
        s.dest_reachable = true;
        s.last_error = None;
        s.last_report = summary;
    });
}

fn record_drain_error(status: &ResyncStatusHandle, now: DateTime<Utc>, e: &ResyncError) {
    update_status(status, |s| {
        s.last_run_at = Some(now);
        s.dest_reachable = true;
        s.last_error = Some(e.to_string());
    });
}

pub async fn run_once(
    source: &PgPool,
    dest: &PgPool,
    dest_url: &str,
    status: &ResyncStatusHandle,
) -> Result<(), ResyncError> {
    let now = Utc::now();

    if primary_tcp_unreachable(dest_url).await {
        tracing::debug!("resync_worker: dest unreachable (fast probe) — skipping drain");
        record_dest_unreachable(status, now);
        return Ok(());
    }

    let engine = ResyncEngine::new(source.clone(), dest.clone());
    match engine.replay_pending().await {
        Ok(report) => {
            log_drain_report(&report);
            record_drain_success(status, now, &report);
            Ok(())
        }
        Err(e) => {
            record_drain_error(status, now, &e);
            Err(e)
        }
    }
}

fn update_status(status: &ResyncStatusHandle, mutate: impl FnOnce(&mut ResyncStatus)) {
    let mut next = ResyncStatus::clone(&status.load());
    mutate(&mut next);
    status.store(Arc::new(next));
}
