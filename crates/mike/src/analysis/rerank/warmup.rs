use std::time::{Duration, Instant};

use tokio::sync::broadcast::{self, error::RecvError};
use tokio_util::sync::CancellationToken;

use super::RerankService;
use crate::state::AppState;
use crate::store::models::{ProxyEvent, RowChangedOp, RowChangedTable};

pub(crate) const WARMUP_WINDOW: Duration = Duration::from_secs(60);

pub(super) struct WarmGate {
    window: Duration,
    last: Option<Instant>,
}

impl WarmGate {
    pub(super) fn new(window: Duration) -> Self {
        Self { window, last: None }
    }

    pub(super) fn permit(&mut self, now: Instant) -> bool {
        let open = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) >= self.window);
        if open {
            self.last = Some(now);
        }
        open
    }
}

pub(crate) fn spawn_rerank_warmup(state: &AppState) {
    let events = state.event_tx.subscribe();
    let service = state.rerank_service();
    let shutdown = state.shutdown();
    state
        .tracker
        .spawn(run(events, service, WARMUP_WINDOW, shutdown));
}

pub(super) async fn run(
    mut events: broadcast::Receiver<ProxyEvent>,
    service: RerankService,
    window: Duration,
    shutdown: CancellationToken,
) {
    let mut gate = WarmGate::new(window);
    loop {
        let received = tokio::select! {
            biased;
            () = shutdown.cancelled() => break,
            received = events.recv() => received,
        };
        match received {
            Ok(event) if starts_a_session(&event) => warm(&service, &mut gate).await,
            Ok(_) | Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => break,
        }
    }
}

fn starts_a_session(event: &ProxyEvent) -> bool {
    matches!(
        event,
        ProxyEvent::RowChanged {
            table: RowChangedTable::Sessions,
            op: RowChangedOp::Insert,
            ..
        }
    )
}

async fn warm(service: &RerankService, gate: &mut WarmGate) {
    if !service.is_enabled() || !gate.permit(Instant::now()) {
        return;
    }
    let health = service.health().await;
    tracing::debug!(?health, "rerank warm-up: asked the reranker for its health");
}
