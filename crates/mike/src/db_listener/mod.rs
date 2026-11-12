use std::time::Duration;

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
    state.tracker.spawn(async move {
        if let Err(err) = run(database_url, event_tx, shutdown).await {
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
    tracing::info!("db_listener: LISTEN proxy.row_changed established");
    emit_resync_required(&event_tx, ResyncReason::ListenerEstablishment);

    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => {
                log_shutdown_signalled();
                return Ok(());
            }
            recv_result = listener.try_recv() => {
                let should_exit =
                    handle_try_recv(recv_result, &event_tx, &mut backoff, &mut in_backoff, &shutdown)
                        .await;
                if should_exit {
                    return Ok(());
                }
            }
        }
    }
}

async fn handle_try_recv(
    recv_result: Result<Option<PgNotification>, sqlx::Error>,
    event_tx: &broadcast::Sender<ProxyEvent>,
    backoff: &mut Duration,
    in_backoff: &mut bool,
    shutdown: &CancellationToken,
) -> bool {
    match recv_result {
        Ok(Some(notification)) => {
            reset_backoff(backoff, in_backoff);
            forward_notification(notification.payload(), event_tx);
            false
        }
        Ok(None) => {
            reset_backoff(backoff, in_backoff);
            tracing::warn!("db_listener: connection lost — reconnected and resubscribed");
            emit_resync_required(event_tx, ResyncReason::Recovery);
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
    listener.listen("proxy.row_changed").await?;
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
