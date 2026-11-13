pub mod correction;
pub mod corrector;
pub mod evidence;
#[cfg(test)]
pub mod evidence_tests;
pub mod followup;
pub mod frames;
pub mod gate;
pub mod proxy_tool;
pub mod scanner;
pub mod stitch;
pub mod support;
#[cfg(test)]
pub mod test_fixture;

use bytes::Bytes;
use futures::{Stream, StreamExt};
use tokio::sync::mpsc;

use self::evidence::{InterceptEvidence, InterceptOutcome};
use self::frames::SseFramer;
use self::gate::{Admit, Cut, StreamGate};
use self::stitch::StitchPoint;
use crate::state::AppState;
use crate::store::models::RequestId;

type ClientTx = mpsc::Sender<Result<Bytes, std::io::Error>>;

pub struct ForwarderCtx {
    pub state: AppState,
    pub upstream_url: String,
    pub forward_headers: reqwest::header::HeaderMap,
    pub original_body: Bytes,
    pub forbidden_words: Vec<String>,
    pub correction_prompt: String,
    pub answers_tool_calls: bool,
    pub original_request_id: RequestId,
    pub session_id: String,
    pub project_path: String,
}

enum Relayed {
    All,
    ClientGone,
    Cut(Cut),
}

pub async fn run_gated_forwarder(
    upstream_resp: reqwest::Response,
    parser_tx: mpsc::Sender<Bytes>,
    body_tx: ClientTx,
    ctx: ForwarderCtx,
) {
    let mut stream = upstream_resp.bytes_stream();
    let mut framer = SseFramer::default();
    let mut gate = StreamGate::new(&ctx.forbidden_words, ctx.answers_tool_calls);

    while let Some(chunk_result) = stream.next().await {
        let chunk = match chunk_result {
            Ok(chunk) => chunk,
            Err(e) => {
                return report_upstream_error(&mut gate, &body_tx, ctx.original_request_id, e)
                    .await;
            }
        };
        let _ = parser_tx.send(chunk.clone()).await;

        match relay(&mut framer, &mut gate, &chunk, &body_tx).await {
            Relayed::All => {}
            Relayed::ClientGone => return,
            Relayed::Cut(cut) => {
                let stitch = gate.stitch_point();
                let tracker = ctx.state.tracker.clone();
                crate::observability::spans::spawn_tracked(
                    &tracker,
                    capture_the_rest(stream, parser_tx),
                );
                return answer_cut(cut, stitch, body_tx, ctx).await;
            }
        }
    }
    drop(parser_tx);

    let mut tail = gate.finish();
    let remainder = framer.take_remainder();
    if !remainder.is_empty() {
        tail.push(remainder);
    }
    send_all(tail, &body_tx).await;
}

async fn relay(
    framer: &mut SseFramer,
    gate: &mut StreamGate,
    chunk: &[u8],
    body_tx: &ClientTx,
) -> Relayed {
    for frame in framer.push(chunk) {
        match gate.admit(frame) {
            Admit::Forward(frames) => {
                if !send_all(frames, body_tx).await {
                    return Relayed::ClientGone;
                }
            }
            Admit::Cut(cut) => return Relayed::Cut(cut),
        }
    }
    Relayed::All
}

async fn send_all(frames: Vec<Bytes>, body_tx: &ClientTx) -> bool {
    for frame in frames {
        if body_tx.send(Ok(frame)).await.is_err() {
            return false;
        }
    }
    true
}

async fn capture_the_rest(
    mut stream: impl Stream<Item = reqwest::Result<Bytes>> + Unpin,
    parser_tx: mpsc::Sender<Bytes>,
) {
    while let Some(Ok(chunk)) = stream.next().await {
        if parser_tx.send(chunk).await.is_err() {
            return;
        }
    }
}

async fn report_upstream_error(
    gate: &mut StreamGate,
    body_tx: &ClientTx,
    request_id: RequestId,
    e: reqwest::Error,
) {
    tracing::warn!(request_id = %request_id, error = %e, "upstream SSE read error");
    if !send_all(gate.finish(), body_tx).await {
        return;
    }
    let _ = body_tx
        .send(Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            e.to_string(),
        )))
        .await;
}

async fn answer_cut(cut: Cut, stitch: StitchPoint, body_tx: ClientTx, ctx: ForwarderCtx) {
    if let Some(index) = stitch.open_block
        && body_tx
            .send(Ok(stitch::block_stop_frame(index)))
            .await
            .is_err()
    {
        return;
    }

    match cut {
        Cut::ToolCall { call, prelude } => {
            record_intercept(&ctx, InterceptOutcome::ProxyToolAnswer, &call.name);
            proxy_tool::run_proxy_tool_loop(
                proxy_tool::ProxyToolLoopCtx {
                    state: ctx.state,
                    upstream_url: ctx.upstream_url,
                    forward_headers: ctx.forward_headers,
                    original_body: ctx.original_body,
                    original_request_id: ctx.original_request_id,
                    session_id: ctx.session_id,
                    project_path: ctx.project_path,
                },
                (*call, *prelude),
                body_tx,
                stitch.next_index,
            )
            .await;
        }
        Cut::Word { word, text } => {
            record_intercept(&ctx, InterceptOutcome::WeaselCorrection, &word);
            correction::run_weasel_correction(
                correction::WeaselCorrectionCtx {
                    state: ctx.state,
                    upstream_url: ctx.upstream_url,
                    forward_headers: ctx.forward_headers,
                    original_body: ctx.original_body,
                    original_request_id: ctx.original_request_id,
                    session_id: ctx.session_id,
                    correction_prompt: ctx.correction_prompt,
                },
                (word, text),
                body_tx,
                stitch.next_index,
            )
            .await;
        }
    }
}

fn record_intercept(ctx: &ForwarderCtx, outcome: InterceptOutcome, subject: &str) {
    evidence::spawn_record(
        &ctx.state,
        InterceptEvidence::new(
            &ctx.session_id,
            ctx.original_request_id,
            outcome,
            Some(subject.to_owned()),
        ),
    );
}

#[cfg(test)]
#[path = "forwarder_tests.rs"]
pub mod forwarder_tests;
