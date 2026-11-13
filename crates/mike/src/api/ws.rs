use std::ops::ControlFlow;

use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message, Utf8Bytes, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;

use crate::state::AppState;
use crate::store::models::ProxyEvent;

#[derive(Serialize)]
struct LagFrame {
    #[serde(rename = "type")]
    kind: &'static str,
    skipped: u64,
}

impl LagFrame {
    fn new(skipped: u64) -> Self {
        Self {
            kind: "Lagged",
            skipped,
        }
    }
}

pub(crate) async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    let rx = state.event_tx.subscribe();
    ws.on_upgrade(move |socket| run_socket(socket, rx))
}

fn serialize_event_payload(event: &ProxyEvent) -> Option<String> {
    match serde_json::to_string(event) {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::error!(error = %e, "failed to serialize ProxyEvent for WS frame");
            None
        }
    }
}

async fn send_text_frame(
    sink: &mut SplitSink<WebSocket, Message>,
    payload: String,
) -> ControlFlow<()> {
    if sink
        .send(Message::Text(Utf8Bytes::from(payload)))
        .await
        .is_err()
    {
        return ControlFlow::Break(());
    }
    ControlFlow::Continue(())
}

async fn forward_event(
    sink: &mut SplitSink<WebSocket, Message>,
    event: ProxyEvent,
) -> ControlFlow<()> {
    let Some(payload) = serialize_event_payload(&event) else {
        return ControlFlow::Continue(());
    };
    let outcome = send_text_frame(sink, payload).await;
    if outcome.is_break() {
        tracing::debug!("ws client gone, exiting forwarder");
    }
    outcome
}

fn emit_broadcast_lag_shed_event() {
    crate::observability::shed::emit(
        crate::observability::shed::MemoryShedEvent {
            event: "memory.shed",
            path: crate::observability::shed::ShedPath::BroadcastLag,
            reason: crate::observability::shed::ShedReason::EvictExisting,
            bytes: 0,
            limit_bytes: 0,
            request_id: None,
            session_id: None,
            note: "broadcast_lag_dropped",
        },
        None,
    );
}

fn serialize_lag_payload(skipped: u64) -> Option<String> {
    match serde_json::to_string(&LagFrame::new(skipped)) {
        Ok(s) => Some(s),
        Err(e) => {
            tracing::error!(error = %e, "failed to serialize lag frame");
            None
        }
    }
}

async fn forward_lag(sink: &mut SplitSink<WebSocket, Message>, skipped: u64) -> ControlFlow<()> {
    emit_broadcast_lag_shed_event();
    tracing::warn!(skipped, "ws subscriber lagged");
    let Some(payload) = serialize_lag_payload(skipped) else {
        return ControlFlow::Continue(());
    };
    send_text_frame(sink, payload).await
}

async fn forward_shutdown_close(sink: &mut SplitSink<WebSocket, Message>) {
    let _ = sink
        .send(Message::Close(Some(CloseFrame {
            code: axum::extract::ws::close_code::NORMAL,
            reason: Utf8Bytes::from_static("server shutdown"),
        })))
        .await;
}

async fn handle_broadcast_result(
    sink: &mut SplitSink<WebSocket, Message>,
    result: Result<ProxyEvent, RecvError>,
) -> ControlFlow<()> {
    match result {
        Ok(event) => forward_event(sink, event).await,
        Err(RecvError::Lagged(skipped)) => forward_lag(sink, skipped).await,
        Err(RecvError::Closed) => {
            forward_shutdown_close(sink).await;
            ControlFlow::Break(())
        }
    }
}

fn log_client_read_error(e: axum::Error) -> ControlFlow<()> {
    tracing::debug!(error = %e, "ws client read error, exiting");
    ControlFlow::Break(())
}

fn log_client_requested_close() -> ControlFlow<()> {
    tracing::debug!("ws client requested close");
    ControlFlow::Break(())
}

fn handle_client_message(client_msg: Option<Result<Message, axum::Error>>) -> ControlFlow<()> {
    match client_msg {
        None => ControlFlow::Break(()),
        Some(Err(e)) => log_client_read_error(e),
        Some(Ok(Message::Close(_))) => log_client_requested_close(),
        Some(Ok(_)) => ControlFlow::Continue(()),
    }
}

pub(crate) async fn run_socket(socket: WebSocket, mut rx: broadcast::Receiver<ProxyEvent>) {
    let (mut sink, mut stream) = socket.split();

    loop {
        let outcome = tokio::select! {
            broadcast_result = rx.recv() => handle_broadcast_result(&mut sink, broadcast_result).await,
            client_msg = stream.next() => handle_client_message(client_msg),
        };
        if outcome.is_break() {
            return;
        }
    }
}

#[cfg(test)]
#[path = "ws_tests.rs"]
mod tests;
