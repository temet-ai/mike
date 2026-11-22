use std::sync::{Arc, OnceLock};
use std::time::Duration;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::read_wait::{Mark, Pending};
use super::validation::ParsedSend;
use crate::api::handlers::{PostSessionMessageBody, PostSessionMessageResponse, tail_for_sender};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;

const ANSWER_BUDGET: Duration = Duration::from_millis(100);
const RETRIES: u32 = 3;
const FIRST_RETRY: Duration = Duration::from_secs(1);

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
    mark: Arc<Mark>,
}

#[derive(Default)]
struct SendWriter {
    queue: OnceLock<mpsc::UnboundedSender<Queued>>,
    pending: Arc<Pending>,
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
    let writer = handle.memo::<SendWriter>();
    let keys = [Some(to), from, job.parsed.thread_id_param];
    let mark = Mark::new(writer.pending.clone(), keys.into_iter().flatten().collect());
    let queued = Queued {
        state: state.clone(),
        handle: handle.clone(),
        job,
        reply,
        mark,
    };
    if let Err(mpsc::error::SendError(queued)) = queue(state, &writer).send(queued) {
        return super::deliver_send(state, handle, &queued.job, &|| {}).await;
    }
    match tokio::time::timeout(ANSWER_BUDGET, answer).await {
        Ok(Ok(answered)) => answered,
        _ => Ok(accepted(state, handle, message_uuid, (to, liveness), from).await),
    }
}

pub(super) fn pending(handle: &PersistHandle) -> Arc<Pending> {
    handle.memo::<SendWriter>().pending.clone()
}

fn queue(state: &AppState, writer: &SendWriter) -> mpsc::UnboundedSender<Queued> {
    writer
        .queue
        .get_or_init(|| {
            let (tx, rx) = mpsc::unbounded_channel();
            state.tracker().spawn(run(rx, state.base.shutdown()));
            tx
        })
        .clone()
}

async fn run(mut rx: mpsc::UnboundedReceiver<Queued>, shutdown: CancellationToken) {
    loop {
        let next = tokio::select! {
            biased;
            next = rx.recv() => next,
            () = shutdown.cancelled() => None,
        };
        match next {
            Some(queued) => settle(queued).await,
            None => break,
        }
    }
    rx.close();
    while let Ok(queued) = rx.try_recv() {
        settle(queued).await;
    }
}

async fn settle(queued: Queued) {
    let Queued {
        state,
        handle,
        job,
        reply,
        mark,
    } = queued;
    let queued = || mark.release();
    let mut reply = Some(reply);
    let mut wait = FIRST_RETRY;
    for attempt in 0..=RETRIES {
        let result = super::deliver_send(&state, &handle, &job, &queued).await;
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
