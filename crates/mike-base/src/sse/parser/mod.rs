use std::time::Instant;

use bytes::Bytes;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use tokio::sync::{broadcast, mpsc};

use crate::store::models::{ProxyEvent, RequestId};
use crate::wire::sse_message::SseReplyAssembler;

pub mod events;
pub mod publish;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

use publish::PublishCtx;

pub async fn run_parser(
    parser_rx: mpsc::Receiver<Bytes>,
    request_id: RequestId,
    event_tx: broadcast::Sender<ProxyEvent>,
    store_tx: mpsc::Sender<ProxyEvent>,
) {
    parse_events(parser_rx, request_id, event_tx, store_tx).await;
}

pub async fn run_parser_returning_reply(
    parser_rx: mpsc::Receiver<Bytes>,
    request_id: RequestId,
    event_tx: broadcast::Sender<ProxyEvent>,
    store_tx: mpsc::Sender<ProxyEvent>,
) -> Option<serde_json::Value> {
    parse_events(parser_rx, request_id, event_tx, store_tx).await
}

async fn parse_events(
    parser_rx: mpsc::Receiver<Bytes>,
    request_id: RequestId,
    event_tx: broadcast::Sender<ProxyEvent>,
    store_tx: mpsc::Sender<ProxyEvent>,
) -> Option<serde_json::Value> {
    let byte_stream =
        tokio_stream::wrappers::ReceiverStream::new(parser_rx).map(Ok::<Bytes, std::io::Error>);
    let mut event_stream = byte_stream.eventsource();

    let mut current_block_type: Option<String> = None;
    let mut reply = SseReplyAssembler::default();

    let mut last_shed_at: Option<Instant> = None;

    while let Some(result) = event_stream.next().await {
        let mut ctx = PublishCtx {
            event_tx: &event_tx,
            store_tx: &store_tx,
            request_id,
            last_shed_at: &mut last_shed_at,
        };
        process_one_event(result, &mut ctx, &mut current_block_type, &mut reply).await;
    }
    tracing::debug!(request_id = %request_id, "parser task finished");
    reply.finish()
}

async fn process_one_event(
    result: Result<eventsource_stream::Event, eventsource_stream::EventStreamError<std::io::Error>>,
    ctx: &mut PublishCtx<'_>,
    current_block_type: &mut Option<String>,
    reply: &mut SseReplyAssembler,
) {
    match result {
        Ok(event) => {
            reply.apply_data(&event.data);
            events::handle_event(&event.event, &event.data, ctx, current_block_type).await;
        }
        Err(e) => {
            tracing::warn!(request_id = %ctx.request_id, error = %e, "SSE parse error, skipping");
        }
    }
}
