use std::sync::OnceLock;
use std::time::Duration;

use axum::Json;
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

use super::validation::ParsedSend;
use crate::api::handlers::{PostSessionMessageBody, PostSessionMessageResponse, tail_for_sender};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;

const ANSWER_BUDGET: Duration = Duration::from_millis(100);
const RETRIES: u32 = 3;
const FIRST_RETRY: Duration = Duration::from_secs(1);
const READ_WAIT: Duration = Duration::from_secs(2);

pub(super) struct Job {
    pub(super) to_session_uuid: uuid::Uuid,
    pub(super) message_uuid: uuid::Uuid,
    pub(super) liveness: Option<crate::workspace::Liveness>,
    pub(super) parsed: ParsedSend,
    pub(super) body: PostSessionMessageBody,
}

struct Queued {
    state: AppState,
    handle: PersistHandle,
    job: Job,
    reply: oneshot::Sender<Result<Response, Response>>,
}

struct SendWriter {
    queue: OnceLock<mpsc::UnboundedSender<Queued>>,
    pending: watch::Sender<usize>,
}

impl Default for SendWriter {
    fn default() -> Self {
        Self {
            queue: OnceLock::new(),
            pending: watch::Sender::new(0),
        }
    }
}

pub(crate) async fn reads_after_sends(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if let Some(handle) = state.persist()
        && !is_send(&request)
    {
        let mut pending = handle.memo::<SendWriter>().pending.subscribe();
        let _ = tokio::time::timeout(READ_WAIT, pending.wait_for(|n| *n == 0)).await;
    }
    next.run(request).await
}

fn is_send(request: &Request) -> bool {
    let path = request.uri().path();
    request.method() == Method::POST
        && path.starts_with("/api/sessions/")
        && path.ends_with("/message")
}

pub(super) async fn accept(
    state: &AppState,
    handle: &PersistHandle,
    job: Job,
) -> Result<Response, Response> {
    let (reply, answer) = oneshot::channel();
    let to = job.to_session_uuid;
    let from = job.parsed.from_session_uuid;
    let message_uuid = job.message_uuid;
    let liveness = job.liveness;
    let queued = Queued {
        state: state.clone(),
        handle: handle.clone(),
        job,
        reply,
    };
    let writer = handle.memo::<SendWriter>();
    writer.pending.send_modify(|n| *n += 1);
    if let Err(mpsc::error::SendError(queued)) = queue(state, &writer).send(queued) {
        writer.pending.send_modify(|n| *n -= 1);
        return super::deliver_send(state, handle, &queued.job).await;
    }
    match tokio::time::timeout(ANSWER_BUDGET, answer).await {
        Ok(Ok(answered)) => answered,
        _ => Ok(accepted(state, handle, message_uuid, (to, liveness), from).await),
    }
}

fn queue(state: &AppState, writer: &std::sync::Arc<SendWriter>) -> mpsc::UnboundedSender<Queued> {
    writer
        .queue
        .get_or_init(|| {
            let (tx, rx) = mpsc::unbounded_channel();
            state
                .tracker()
                .spawn(run(rx, writer.clone(), state.base.shutdown()));
            tx
        })
        .clone()
}

async fn run(
    mut rx: mpsc::UnboundedReceiver<Queued>,
    writer: std::sync::Arc<SendWriter>,
    shutdown: CancellationToken,
) {
    loop {
        let next = tokio::select! {
            biased;
            next = rx.recv() => next,
            () = shutdown.cancelled() => None,
        };
        match next {
            Some(queued) => {
                settle(queued).await;
                writer.pending.send_modify(|n| *n -= 1);
            }
            None => break,
        }
    }
    rx.close();
    while let Ok(queued) = rx.try_recv() {
        settle(queued).await;
        writer.pending.send_modify(|n| *n -= 1);
    }
}

async fn settle(queued: Queued) {
    let Queued {
        state,
        handle,
        job,
        reply,
    } = queued;
    let mut reply = Some(reply);
    let mut wait = FIRST_RETRY;
    for attempt in 0..=RETRIES {
        let result = super::deliver_send(&state, &handle, &job).await;
        let status = match &result {
            Ok(response) | Err(response) => response.status(),
        };
        let answered = reply.take().is_some_and(|reply| reply.send(result).is_ok());
        if answered || status.is_success() {
            return;
        }
        if !status.is_server_error() || attempt == RETRIES {
            tracing::error!(
                message_uuid = %job.message_uuid,
                to_session_uuid = %job.to_session_uuid,
                %status,
                attempts = attempt + 1,
                "post_session_message: an accepted message was not queued"
            );
            return;
        }
        tokio::time::sleep(wait).await;
        wait *= 2;
    }
}

async fn accepted(
    state: &AppState,
    handle: &PersistHandle,
    message_uuid: uuid::Uuid,
    (to_session_uuid, liveness): (uuid::Uuid, Option<crate::workspace::Liveness>),
    from_session_uuid: Option<uuid::Uuid>,
) -> Response {
    let delivery_state = if crate::mcp_channel::listeners().is_live(to_session_uuid) {
        "channel"
    } else {
        "awaiting_eligible_turn"
    };
    let since_you_last_looked = tail_for_sender(state, handle, from_session_uuid).await;
    (
        StatusCode::CREATED,
        Json(crate::api::changes::WithTail {
            report: PostSessionMessageResponse {
                message_uuid,
                to_session_uuid,
                from_session_uuid,
                queued_at: chrono::Utc::now().to_rfc3339(),
                recipient_liveness: liveness.map(crate::workspace::Liveness::as_str),
                recipient_last_request_at: None,
                recipient_last_eligible_turn_at: None,
                delivery_state: Some(delivery_state),
            },
            since_you_last_looked,
        }),
    )
        .into_response()
}
