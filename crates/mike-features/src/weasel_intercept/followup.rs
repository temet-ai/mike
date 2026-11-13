use bytes::Bytes;
use futures::StreamExt;
use tokio::sync::mpsc;

use super::frames::SseFramer;
use super::stitch;
use crate::state::AppState;
use crate::store::models::RequestId;

type ClientTx = mpsc::Sender<Result<Bytes, std::io::Error>>;

pub fn spawn_stitched_tee(
    state: &AppState,
    upstream_resp: reqwest::Response,
    body_tx: ClientTx,
    request_id: RequestId,
    next_index: u64,
) {
    let (parser_tx, parser_rx) = mpsc::channel::<Bytes>(64);
    let (store_event_tx, store_event_rx) = tokio::sync::mpsc::channel(512);

    crate::observability::spans::spawn_tracked(
        &state.tracker,
        stitch_to_client_and_parser(upstream_resp, body_tx, parser_tx, next_index),
    );
    crate::observability::spans::spawn_tracked(
        &state.tracker,
        crate::sse::parser::run_parser(
            parser_rx,
            request_id,
            state.event_tx.clone(),
            store_event_tx,
        ),
    );
    crate::store::updater::spawn_store_updater(
        crate::store::updater::StoreUpdaterSpawnArgs {
            store: state.store.clone(),
            event_rx: store_event_rx,
            request_id,
            in_flight: state.in_flight.clone(),
            persist: state.persist().cloned(),
            response_hook: Some(std::sync::Arc::new(
                crate::memory_harvester::HarvesterHook::new(state.config.harvester.clone()),
            )),
        },
        &state.tracker,
    );
}

async fn stitch_to_client_and_parser(
    upstream_resp: reqwest::Response,
    body_tx: ClientTx,
    parser_tx: mpsc::Sender<Bytes>,
    next_index: u64,
) {
    let status = upstream_resp.status();
    if !status.is_success() {
        let body = upstream_resp.bytes().await.unwrap_or_default();
        let _ = parser_tx.send(body.clone()).await;
        let _ = body_tx
            .send(Ok(stitch::upstream_error_frame(status.as_u16(), &body)))
            .await;
        return;
    }

    let mut stream = upstream_resp.bytes_stream();
    let mut framer = SseFramer::default();
    let mut parser_alive = true;
    while let Some(chunk_result) = stream.next().await {
        let chunk = match chunk_result {
            Ok(chunk) => chunk,
            Err(e) => {
                let _ = body_tx
                    .send(Err(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        e.to_string(),
                    )))
                    .await;
                return;
            }
        };
        if parser_alive && parser_tx.send(chunk.clone()).await.is_err() {
            parser_alive = false;
        }
        if !send_stitched(&mut framer, &chunk, next_index, &body_tx).await {
            return;
        }
    }
    let remainder = framer.take_remainder();
    if !remainder.is_empty() {
        let _ = body_tx.send(Ok(remainder)).await;
    }
}

async fn send_stitched(
    framer: &mut SseFramer,
    chunk: &[u8],
    next_index: u64,
    body_tx: &ClientTx,
) -> bool {
    for frame in framer.push(chunk) {
        let Some(out) = stitch::onto_the_first_reply(frame, next_index) else {
            continue;
        };
        if body_tx.send(Ok(out)).await.is_err() {
            return false;
        }
    }
    true
}
