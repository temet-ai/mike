use crate::state::AppState;

pub(super) async fn run_startup_sequence(state: &AppState) {
    warn_on_non_loopback_binds(state);

    crate::startup::run_crash_recovery(state).await;

    crate::startup::record_build_identity(state).await;

    install_fleet_files();

    crate::startup::spawn_bundle_retry(state);

    crate::feature_presets::apply_pending_at_start(state).await;

    crate::startup::run_startup_session_sweep(state).await;

    crate::startup::run_mailbox_reload(state).await;

    crate::startup::run_composable_session_warmup(state).await;

    crate::observability::banner::emit_startup_banner(state).await;

    crate::change_feed::spawn_change_feed(state);

    crate::db_listener::spawn_listener(state);
}

fn install_fleet_files() {
    let home = crate::launch::fleet::bundle::home();
    if let Err(problem) = crate::launch::fleet::bundle::install(&home) {
        warn_fleet_files_not_installed(&problem);
    }
}

fn warn_fleet_files_not_installed(problem: &str) {
    tracing::warn!(%problem, "mike could not install its fleet files at start");
}

fn warn_on_non_loopback_binds(state: &AppState) {
    let proxy_addr = &state.config().proxy_listen_addr;
    if !is_loopback_addr(proxy_addr) {
        warn_proxy_bind_non_loopback(proxy_addr);
    }
    let inspect_addr = &state.config().inspect_listen_addr;
    if !is_loopback_addr(inspect_addr) {
        warn_inspect_bind_non_loopback(inspect_addr);
    }
}

fn warn_proxy_bind_non_loopback(proxy_addr: &str) {
    tracing::warn!(
        proxy_listen_addr = %proxy_addr,
        "binding proxy to non-loopback address — Claude traffic will be network-accessible"
    );
}

fn warn_inspect_bind_non_loopback(inspect_addr: &str) {
    tracing::warn!(
        inspect_listen_addr = %inspect_addr,
        "binding inspect API to non-loopback address — inspection API will be network-accessible"
    );
}

fn is_loopback_addr(addr: &str) -> bool {
    addr.starts_with("127.") || addr.starts_with("localhost")
}
