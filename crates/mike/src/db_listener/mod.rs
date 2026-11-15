mod config_reload;

use std::time::Duration;

pub use config_reload::{CONFIG_CHANNEL, ConfigReloader};
use serde::Deserialize;
use sqlx::postgres::{PgListener, PgNotification};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::state::AppState;
use crate::store::models::{ProxyEvent, ResyncReason, RowChangedOp, RowChangedTable};
use crate::store::persistence::ActiveDbTarget;

const BACKOFF_INITIAL: Duration = Duration::from_millis(100);

const BACKOFF_MAX: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize)]
pub(crate) struct RowChangedPayload {
    pub table: RowChangedTable,
    pub id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    pub op: RowChangedOp,
}

pub fn spawn_listener(state: &AppState) {
    let Some(persist) = state.persist.as_ref() else {
        return;
    };

    let url = match persist.active_target() {
        ActiveDbTarget::LocalFallback => state.config.local_database_url.clone(),
        ActiveDbTarget::Remote => state.config.database_url.clone(),
    };
    let Some(database_url) = url else {
        tracing::warn!(
            active_target = ?persist.active_target(),
            "db_listener: no URL for active persist target — skipping listener"
        );
        return;
    };

    let event_tx = state.event_tx.clone();
    let shutdown = state.shutdown.clone();
    let reloader = config_reload::spawn_config_reloader(state);
    state.tracker.spawn(async move {
        if let Err(err) = run(database_url, event_tx, reloader, shutdown).await {
            tracing::error!(
                error = %err,
                "db_listener task exited with error — cross-session live updates disabled until proxy restart"
            );
        }
    });
}

pub async fn run(
    database_url: String,
    event_tx: broadcast::Sender<ProxyEvent>,
    reloader: ConfigReloader,
    shutdown: CancellationToken,
) -> anyhow::Result<()> {
    let mut backoff: Duration = BACKOFF_INITIAL;
    let mut in_backoff: bool = false;

    let Some(mut listener) =
        establish_listener(&database_url, &mut backoff, &mut in_backoff, &shutdown).await
    else {
        return Ok(());
    };
    reset_backoff(&mut backoff, &mut in_backoff);
    tracing::info!("db_listener: LISTEN proxy.row_changed and {CONFIG_CHANNEL} established");
    emit_resync_required(&event_tx, ResyncReason::ListenerEstablishment);
    reloader.request();

    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => {
                log_shutdown_signalled();
                return Ok(());
            }
            recv_result = listener.try_recv() => {
                let outlets = Outlets { event_tx: &event_tx, reloader: &reloader };
                let should_exit =
                    handle_try_recv(recv_result, outlets, &mut backoff, &mut in_backoff, &shutdown)
                        .await;
                if should_exit {
                    return Ok(());
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Outlets<'a> {
    event_tx: &'a broadcast::Sender<ProxyEvent>,
    reloader: &'a ConfigReloader,
}

fn route_notification(notification: &PgNotification, outlets: Outlets<'_>) {
    if notification.channel() == CONFIG_CHANNEL {
        outlets.reloader.request();
    } else {
        forward_notification(notification.payload(), outlets.event_tx);
    }
}

async fn handle_try_recv(
    recv_result: Result<Option<PgNotification>, sqlx::Error>,
    outlets: Outlets<'_>,
    backoff: &mut Duration,
    in_backoff: &mut bool,
    shutdown: &CancellationToken,
) -> bool {
    match recv_result {
        Ok(Some(notification)) => {
            reset_backoff(backoff, in_backoff);
            route_notification(&notification, outlets);
            false
        }
        Ok(None) => {
            reset_backoff(backoff, in_backoff);
            tracing::warn!("db_listener: connection lost — reconnected and resubscribed");
            emit_resync_required(outlets.event_tx, ResyncReason::Recovery);
            outlets.reloader.request();
            false
        }
        Err(e) => {
            if back_off_after_recv_error(e, backoff, shutdown).await {
                return true;
            }
            *in_backoff = true;
            false
        }
    }
}

async fn establish_listener(
    database_url: &str,
    backoff: &mut Duration,
    in_backoff: &mut bool,
    shutdown: &CancellationToken,
) -> Option<PgListener> {
    loop {
        match connect_and_listen(database_url).await {
            Ok(listener) => return Some(listener),
            Err(err) => {
                log_connect_error(&err, *backoff);
                if wait_and_double_backoff(backoff, shutdown).await {
                    log_shutdown_during_backoff();
                    return None;
                }
                *in_backoff = true;
            }
        }
    }
}

async fn connect_and_listen(database_url: &str) -> Result<PgListener, sqlx::Error> {
    let mut listener = PgListener::connect(database_url).await?;
    listener
        .listen_all(["proxy.row_changed", CONFIG_CHANNEL])
        .await?;
    Ok(listener)
}

fn emit_resync_required(event_tx: &broadcast::Sender<ProxyEvent>, reason: ResyncReason) {
    let _ = event_tx.send(ProxyEvent::ResyncRequired { reason });
}

fn reset_backoff(backoff: &mut Duration, in_backoff: &mut bool) {
    if *in_backoff {
        *backoff = BACKOFF_INITIAL;
        *in_backoff = false;
    }
}

fn log_shutdown_signalled() {
    tracing::debug!("db_listener: shutdown signalled, exiting");
}

async fn wait_and_double_backoff(backoff: &mut Duration, shutdown: &CancellationToken) -> bool {
    let exit = tokio::select! {
        biased;
        _ = shutdown.cancelled() => true,
        _ = tokio::time::sleep(*backoff) => false,
    };
    *backoff = (*backoff * 2).min(BACKOFF_MAX);
    exit
}

async fn back_off_after_recv_error(
    err: sqlx::Error,
    backoff: &mut Duration,
    shutdown: &CancellationToken,
) -> bool {
    log_recv_error(&err, *backoff);
    let exit = wait_and_double_backoff(backoff, shutdown).await;
    if exit {
        log_shutdown_during_backoff();
    }
    exit
}

fn log_recv_error(err: &sqlx::Error, backoff: Duration) {
    tracing::warn!(
        error = %err,
        backoff_ms = backoff.as_millis() as u64,
        "db_listener: PgListener::try_recv returned error; backing off before retry"
    );
}

fn log_connect_error(err: &sqlx::Error, backoff: Duration) {
    tracing::warn!(
        error = %err,
        backoff_ms = backoff.as_millis() as u64,
        "db_listener: connect/LISTEN failed; backing off before retry"
    );
}

fn log_shutdown_during_backoff() {
    tracing::debug!("db_listener: shutdown during back-off, exiting");
}

fn forward_notification(payload: &str, event_tx: &broadcast::Sender<ProxyEvent>) {
    let parsed: RowChangedPayload = match serde_json::from_str(payload) {
        Ok(p) => p,
        Err(err) => {
            tracing::warn!(
                error = %err,
                payload,
                "db_listener: failed to parse row_changed payload — skipping"
            );
            emit_resync_required(event_tx, ResyncReason::UnusableNotification);
            return;
        }
    };
    let event = ProxyEvent::RowChanged {
        table: parsed.table,
        id: parsed.id,
        session_id: parsed.session_id,
        op: parsed.op,
    };
    let _ = event_tx.send(event);
}

#[cfg(test)]
mod tests;
