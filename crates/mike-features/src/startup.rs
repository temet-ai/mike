use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::mailbox::ReloadedMailboxRow;
use crate::store::persistence::{PersistError, PersistHandle};

pub async fn run_crash_recovery(state: &crate::state::AppState) {
    if state.config().smoke_run {
        tracing::info!("smoke-run: skipping crash-recovery sweep");
        return;
    }
    let Some(persist) = state.persist() else {
        return;
    };
    log_crash_recovery_result(persist.recover_stale_pending_rows().await);
}

fn should_record_build(config: &crate::config::ProxyConfig) -> bool {
    !config.smoke_run
}

pub async fn record_build_identity(state: &crate::state::AppState) {
    if !should_record_build(state.config()) {
        log_smoke_run_skips_build();
        return;
    }
    let Some(persist) = state.persist() else {
        return;
    };
    let build = crate::change_log::current_build();
    let defaults = crate::change_log::code_defaults(state.config());
    let recorded = crate::logged::ok_or_warn(
        persist.record_build_if_changed(build, defaults).await,
        "startup.record_build_identity",
    )
    .flatten();
    if let Some(id) = recorded {
        log_build_recorded(id, build.sha);
    }
}

fn log_smoke_run_skips_build() {
    tracing::info!("smoke-run: skipping build identity record");
}

fn log_build_recorded(change_id: i64, sha: &str) {
    tracing::info!(change_id, sha, "recorded new build on the change log");
}

pub fn spawn_bundle_retry(state: &crate::state::AppState) {
    if let Some(publisher) = state
        .steerer_service()
        .and_then(|service| service.bundle_publisher())
    {
        publisher.spawn_retry(&state.tracker, state.shutdown());
    }
}

fn log_crash_recovery_result(result: Result<u64, PersistError>) {
    match result {
        Ok(n) if n > 0 => log_crash_recovery_recovered(n),
        Ok(_) => {}
        Err(e) => log_crash_recovery_failed(&e),
    }
}

fn log_crash_recovery_recovered(recovered: u64) {
    tracing::info!(recovered, "closed stale in-flight rows from prior crash");
}

fn log_crash_recovery_failed(e: &PersistError) {
    tracing::warn!(
        error = %e,
        "crash-recovery sweep failed — stale Pending rows may remain"
    );
}

pub async fn run_startup_session_sweep(state: &crate::state::AppState) {
    let cfg = state.config();
    if cfg.smoke_run {
        tracing::info!("smoke-run: skipping startup session sweep");
        return;
    }
    if !cfg.sweep_at_startup {
        return;
    }
    let Some(persist) = state.persist() else {
        return;
    };
    let default_ttl = crate::store::retention::RetentionPolicy::parse_duration(&cfg.default_ttl);
    let started = std::time::Instant::now();
    let result = persist
        .sweep_expired_sessions(default_ttl, chrono::Utc::now())
        .await;
    log_startup_session_sweep_result(result, default_ttl, started.elapsed());
}

fn log_startup_session_sweep_result(
    result: Result<u64, PersistError>,
    default_ttl: std::time::Duration,
    elapsed: std::time::Duration,
) {
    match result {
        Ok(removed) => log_startup_session_sweep_complete(removed, default_ttl, elapsed),
        Err(e) => log_startup_session_sweep_failed(&e),
    }
}

fn log_startup_session_sweep_complete(
    removed: u64,
    default_ttl: std::time::Duration,
    elapsed: std::time::Duration,
) {
    tracing::info!(
        sessions_removed = removed,
        default_ttl = ?default_ttl,
        elapsed_ms = elapsed.as_millis() as u64,
        "startup session sweep complete"
    );
}

fn log_startup_session_sweep_failed(e: &PersistError) {
    tracing::error!(
        error = %e,
        "startup session sweep failed — continuing boot"
    );
}

const MAILBOX_RELOAD_LIMIT: i64 = 256;

const MAILBOX_STALE_CLAIM_AFTER: chrono::Duration = chrono::Duration::minutes(5);

pub async fn run_mailbox_reload(state: &crate::state::AppState) {
    if state.config().smoke_run {
        log_mailbox_reload_skipped_smoke_run();
        return;
    }
    let Some(persist) = state.persist() else {
        return;
    };

    let mut rows = fetch_all_unclaimed_mailbox_rows(persist).await;
    rows.extend(fetch_all_stale_claimed_mailbox_rows(persist).await);
    rows.extend(fetch_all_redelivery_mailbox_rows(persist).await);

    if rows.is_empty() {
        return;
    }

    let enqueued = rows.len();
    enqueue_reminders_for_reloaded_rows(state, &rows).await;
    log_mailbox_reload_enqueued(enqueued);
}

fn log_mailbox_reload_skipped_smoke_run() {
    tracing::info!("smoke-run: skipping mailbox reload");
}

fn log_mailbox_reload_enqueued(enqueued: usize) {
    tracing::info!(
        enqueued,
        "startup mailbox reload: re-enqueued pending cross-session messages"
    );
}

async fn fetch_all_unclaimed_mailbox_rows(persist: &PersistHandle) -> Vec<ReloadedMailboxRow> {
    match persist
        .launch()
        .fetch_all_pending_unclaimed(MAILBOX_RELOAD_LIMIT)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "startup mailbox reload: pending-unclaimed query failed — continuing boot"
            );
            Vec::new()
        }
    }
}

async fn fetch_all_stale_claimed_mailbox_rows(persist: &PersistHandle) -> Vec<ReloadedMailboxRow> {
    let cutoff = chrono::Utc::now() - MAILBOX_STALE_CLAIM_AFTER;
    match persist
        .launch()
        .fetch_all_stale_claimed(cutoff, MAILBOX_RELOAD_LIMIT)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "startup mailbox reload: stale-claim query failed — continuing with unclaimed rows only"
            );
            Vec::new()
        }
    }
}

async fn fetch_all_redelivery_mailbox_rows(persist: &PersistHandle) -> Vec<ReloadedMailboxRow> {
    match persist
        .launch()
        .fetch_all_pending_redelivery(MAILBOX_RELOAD_LIMIT)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "startup mailbox reload: pending-redelivery query failed — continuing without it"
            );
            Vec::new()
        }
    }
}

async fn enqueue_reminders_for_reloaded_rows(
    state: &crate::state::AppState,
    rows: &[ReloadedMailboxRow],
) {
    let mut store = state.store.write().await;
    for entry in rows {
        crate::injection::enqueue_if_admitted(
            &state.injection_gate,
            &mut store,
            &entry.row.to_session_uuid.to_string(),
            entry.row.to_reminder(),
        );
    }
}

pub async fn run_composable_session_warmup(state: &crate::state::AppState) {
    let Some(persist) = state.persist() else {
        return;
    };

    let rows = fetch_composable_session_seed_uuids(persist).await;
    if rows.is_empty() {
        return;
    }

    let seeded = rows.len();
    let mut store = state.store.write().await;
    for session_uuid in rows {
        store.seed_composable_session(&session_uuid.to_string());
    }
    drop(store);

    tracing::info!(
        seeded,
        "startup composable-session warmup: seeded composable_session_index from ledger"
    );
}

async fn fetch_composable_session_seed_uuids(persist: &PersistHandle) -> Vec<uuid::Uuid> {
    match sqlx::query_scalar(
        "SELECT DISTINCT s.session_uuid
         FROM system_prompts.session_prompts sp
         JOIN sessions.sessions s ON s.id = sp.session_id
         LEFT JOIN requests.requests r ON r.id = sp.request_id
         LEFT JOIN requests.turn_origins t ON t.id = r.turn_origin_id
         WHERE sp.lane = 'main'
           AND (sp.altered = true OR t.code = 'primary_first')",
    )
    .fetch_all(persist.pool())
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "startup composable-session warmup: ledger query failed — continuing boot"
            );
            Vec::new()
        }
    }
}

pub async fn reload_mailbox_for_session(
    state: &crate::state::AppState,
    session_uuid: uuid::Uuid,
) {
    let Some(persist) = state.persist() else {
        return;
    };

    let mut rows = fetch_unclaimed_mailbox_rows_for(persist, session_uuid).await;
    rows.extend(fetch_stale_claimed_mailbox_rows_for(persist, session_uuid).await);
    rows.extend(fetch_redelivery_mailbox_rows_for(persist, session_uuid).await);

    if rows.is_empty() {
        return;
    }

    enqueue_reminders_for_reloaded_rows(state, &rows).await;
}

async fn fetch_unclaimed_mailbox_rows_for(
    persist: &PersistHandle,
    session_uuid: uuid::Uuid,
) -> Vec<ReloadedMailboxRow> {
    match persist
        .launch()
        .fetch_pending_unclaimed(session_uuid, MAILBOX_RELOAD_LIMIT)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(
                error = %e,
                session_id = %session_uuid,
                "first-seen mailbox reload: pending-unclaimed query failed"
            );
            Vec::new()
        }
    }
}

async fn fetch_stale_claimed_mailbox_rows_for(
    persist: &PersistHandle,
    session_uuid: uuid::Uuid,
) -> Vec<ReloadedMailboxRow> {
    let cutoff = chrono::Utc::now() - MAILBOX_STALE_CLAIM_AFTER;
    match persist
        .launch()
        .fetch_stale_claimed(session_uuid, cutoff, MAILBOX_RELOAD_LIMIT)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(
                error = %e,
                session_id = %session_uuid,
                "first-seen mailbox reload: stale-claim query failed"
            );
            Vec::new()
        }
    }
}

async fn fetch_redelivery_mailbox_rows_for(
    persist: &PersistHandle,
    session_uuid: uuid::Uuid,
) -> Vec<ReloadedMailboxRow> {
    match persist
        .launch()
        .fetch_pending_redelivery(session_uuid, MAILBOX_RELOAD_LIMIT)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(
                error = %e,
                session_id = %session_uuid,
                "first-seen mailbox reload: pending-redelivery query failed"
            );
            Vec::new()
        }
    }
}

#[cfg(test)]
pub mod build_identity_tests {
    use super::should_record_build;

    #[test]
    fn a_smoke_run_records_no_build() {
        let mut config = crate::config::ProxyConfig::for_tests_in_memory();
        assert!(should_record_build(&config));
        config.smoke_run = true;
        assert!(!should_record_build(&config));
    }
}
