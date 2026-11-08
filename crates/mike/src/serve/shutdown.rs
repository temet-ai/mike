use anyhow::Context;
use tokio_util::sync::CancellationToken;
use tracing::info;

use super::listeners::ServeTargets;

const GRACEFUL_DRAIN_SECS: u64 = 10;

const SHUTDOWN_HARD_DEADLINE_SECS: u64 = 60;

const HARD_DEADLINE_EXIT_CODE: i32 = 1;

pub(super) async fn serve_until_shutdown(targets: ServeTargets) -> anyhow::Result<()> {
    let shutdown_token = targets.shutdown_token.clone();

    let signal_task = tokio::spawn(install_signal_handler(shutdown_token.clone()));

    spawn_hard_deadline_backstop(shutdown_token.clone());

    let result = race_serve_against_drain_deadline(targets, shutdown_token.clone()).await;

    shutdown_token.cancel();
    let _ = signal_task.await;
    result
}

fn spawn_hard_deadline_backstop(shutdown_token: CancellationToken) {
    tokio::spawn(async move {
        shutdown_token.cancelled().await;
        tokio::time::sleep(std::time::Duration::from_secs(SHUTDOWN_HARD_DEADLINE_SECS)).await;
        tracing::error!(
            "shutdown exceeded {SHUTDOWN_HARD_DEADLINE_SECS}s hard deadline — \
             forcing process exit"
        );
        std::process::exit(HARD_DEADLINE_EXIT_CODE);
    });
}

async fn race_serve_against_drain_deadline(
    targets: ServeTargets,
    shutdown_token: CancellationToken,
) -> anyhow::Result<()> {
    let proxy_shutdown = shutdown_token.clone();
    let inspect_shutdown = shutdown_token.clone();
    let spa_shutdown = shutdown_token.clone();

    let proxy_serve = axum::serve(targets.proxy_listener, targets.proxy_app)
        .with_graceful_shutdown(async move {
            proxy_shutdown.cancelled().await;
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        });
    let inspect_serve = axum::serve(targets.inspect_listener, targets.inspect_app)
        .with_graceful_shutdown(async move {
            inspect_shutdown.cancelled().await;
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        });
    let spa_serve = async move {
        match targets.spa_listener {
            Some(listener) => axum::serve(listener, targets.spa_app)
                .with_graceful_shutdown(async move {
                    spa_shutdown.cancelled().await;
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                })
                .await
                .context("SPA bind failed"),
            None => {
                spa_shutdown.cancelled().await;
                Ok(())
            }
        }
    };

    let serve_join = async {
        tokio::try_join!(
            async { proxy_serve.await.context("proxy bind failed") },
            async { inspect_serve.await.context("inspect bind failed") },
            spa_serve,
        )
    };
    let drain_deadline = {
        let tok = shutdown_token.clone();
        async move {
            tok.cancelled().await;
            tokio::time::sleep(std::time::Duration::from_secs(GRACEFUL_DRAIN_SECS)).await;
        }
    };

    let result = tokio::select! {
        joined = serve_join => joined,
        _ = drain_deadline => {
            tracing::warn!(
                "graceful connection drain exceeded {GRACEFUL_DRAIN_SECS}s after shutdown — \
                 proceeding (in-flight connections force-closed)"
            );
            Ok(((), (), ()))
        }
    };

    result.map(|_| ())
}

#[cfg(unix)]
async fn wait_for_sigterm() {
    use tokio::signal;

    match signal::unix::signal(signal::unix::SignalKind::terminate()) {
        Ok(mut sig) => {
            sig.recv().await;
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to install SIGTERM handler; relying on ctrl-c only");
            std::future::pending::<()>().await;
        }
    }
}

#[cfg(not(unix))]
async fn wait_for_sigterm() {
    std::future::pending::<()>().await;
}

#[cfg(unix)]
async fn wait_for_sighup() {
    use tokio::signal;

    match signal::unix::signal(signal::unix::SignalKind::hangup()) {
        Ok(mut sig) => {
            sig.recv().await;
        }
        Err(_) => {
            std::future::pending::<()>().await;
        }
    }
}

#[cfg(not(unix))]
async fn wait_for_sighup() {
    std::future::pending::<()>().await;
}

async fn install_signal_handler(cancel: CancellationToken) {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c().await.ok();
    };

    let trigger = tokio::select! {
        _ = ctrl_c => "SIGINT (Ctrl-C / foreground terminal interrupt)",
        _ = wait_for_sigterm() => "SIGTERM (kill / restart script / process manager / parent-process exit)",
        _ = wait_for_sighup() => "SIGHUP (controlling terminal closed / parent shell exited)",
        _ = cancel.cancelled() => {
            log_shutdown_triggered_internally();
            return;
        }
    };

    log_shutdown_signal_received(trigger);
    cancel.cancel();
}

fn log_shutdown_triggered_internally() {
    info!(
        "shutdown: cancellation token fired internally (a serve task exited \
         or the retention pruner) — no OS signal involved"
    );
}

fn log_shutdown_signal_received(trigger: &str) {
    info!(signal = %trigger, "shutdown signal received");
}
