use std::collections::HashMap;
use std::ops::ControlFlow;
use std::time::Duration;

use tracing::{info, warn};

use super::context::{DebounceLedger, FleetArgs, FleetCtx, RequestBuffer};
use super::triggers;
use crate::circuit_breaker::CircuitBreaker;
use crate::steerer::gate::strip_harness_scaffold;
use crate::store::models::{ProxyEvent, RequestId};

const CIRCUIT_BREAKER_THRESHOLD: u32 = 10;

const CIRCUIT_BREAKER_COOLDOWN_BASE_SECS: u64 = 30;

const CIRCUIT_BREAKER_COOLDOWN_MAX_SECS: u64 = 600;

enum NextEvent {
    Event(Box<ProxyEvent>),
    Skip,
    Stop,
}

fn log_broadcast_lagged(skipped: u64) {
    warn!(skipped, "steerer fleet: broadcast lagged — skipped events");
}

fn log_broadcast_closed() {
    info!("steerer fleet: broadcast closed");
}

fn log_steerer_disabled_via_toggle() {
    info!(
        "steerer fleet: disabled via feature toggle — exiting (a UI flip back on needs a restart)"
    );
}

async fn recv_next_event(
    rx: &mut tokio::sync::broadcast::Receiver<ProxyEvent>,
    ctx: &FleetCtx,
) -> NextEvent {
    let event = match rx.recv().await {
        Ok(e) => e,
        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
            log_broadcast_lagged(n);
            return NextEvent::Skip;
        }
        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
            log_broadcast_closed();
            return NextEvent::Stop;
        }
    };
    if !ctx.toggles.load().steerer {
        log_steerer_disabled_via_toggle();
        return NextEvent::Stop;
    }
    NextEvent::Event(Box::new(event))
}

async fn fleet_tick(
    rx: &mut tokio::sync::broadcast::Receiver<ProxyEvent>,
    ctx: &FleetCtx,
    debounce: &mut HashMap<(String, i64), DebounceLedger>,
    request_buf: &mut HashMap<u64, RequestBuffer>,
    breaker: &mut CircuitBreaker,
) -> ControlFlow<()> {
    tokio::select! {
        biased;
        _ = ctx.shutdown.cancelled() => {
            info!("steerer fleet: shutdown");
            ControlFlow::Break(())
        }
        next = recv_next_event(rx, ctx) => {
            match next {
                NextEvent::Event(event) => {
                    handle_event(*event, ctx, debounce, request_buf, breaker).await;
                    ControlFlow::Continue(())
                }
                NextEvent::Skip => ControlFlow::Continue(()),
                NextEvent::Stop => ControlFlow::Break(()),
            }
        }
    }
}

pub async fn run_fleet(args: FleetArgs) {
    let FleetArgs {
        mut rx,
        event_tx,
        store,
        persist,
        steerer_registry,
        llm,
        key_slot,
        toggles,
        config,
        shutdown,
        remote_control_streams,
        injection_gate,
        settings,
    } = args;

    let ctx = FleetCtx {
        event_tx,
        store,
        persist,
        steerer_registry,
        llm,
        key_slot,
        toggles,
        config,
        shutdown,
        remote_control_streams,
        injection_gate,
        settings,
    };

    let mut debounce: HashMap<(String, i64), DebounceLedger> = HashMap::new();
    let mut request_buf: HashMap<u64, RequestBuffer> = HashMap::new();

    let mut breaker = CircuitBreaker::new(
        CIRCUIT_BREAKER_THRESHOLD,
        Duration::from_secs(CIRCUIT_BREAKER_COOLDOWN_BASE_SECS),
        Duration::from_secs(CIRCUIT_BREAKER_COOLDOWN_MAX_SECS),
    );

    info!("steerer fleet: started");

    while fleet_tick(&mut rx, &ctx, &mut debounce, &mut request_buf, &mut breaker)
        .await
        .is_continue()
    {}
}

async fn handle_response_complete(
    id: RequestId,
    stop_reason: Option<String>,
    ctx: &FleetCtx,
    debounce: &mut HashMap<(String, i64), DebounceLedger>,
    request_buf: &mut HashMap<u64, RequestBuffer>,
    breaker: &mut CircuitBreaker,
) {
    let raw_id = id.as_u64();
    let entry = request_buf.entry(raw_id).or_default();
    entry.response_complete = true;
    entry.stop_reason = stop_reason;

    {
        let guard = ctx.store.read().await;
        if let Some(req) = guard.get(&id) {
            entry.session_id = Some(req.session_id.clone());
            entry.last_user_message = Some(strip_harness_scaffold(&req.last_user_message));
        }
    }

    if let Some(sid) = entry.session_id.clone() {
        let registry = ctx.steerer_registry.load_full();
        for s in registry.steerers.iter() {
            debounce.entry((sid.clone(), s.id)).or_default().tick();
        }
    }

    if entry.is_ready() {
        triggers::evaluate_and_maybe_fire(raw_id, ctx, debounce, request_buf, breaker).await;
    }
}

struct DiagnosticReadyEvent {
    id: RequestId,
    count: usize,
    accumulated_text: String,
}

async fn handle_diagnostic_ready(
    event: DiagnosticReadyEvent,
    ctx: &FleetCtx,
    debounce: &mut HashMap<(String, i64), DebounceLedger>,
    request_buf: &mut HashMap<u64, RequestBuffer>,
    breaker: &mut CircuitBreaker,
) {
    let DiagnosticReadyEvent {
        id,
        count,
        accumulated_text,
    } = event;
    let raw_id = id.as_u64();
    let entry = request_buf.entry(raw_id).or_default();

    if !accumulated_text.is_empty() {
        entry.accumulated_text = Some(accumulated_text);
    }

    if count == 0 {
        entry.diagnostics = Some(Vec::new());
    } else {
        let diagnostics = {
            let guard = ctx.store.read().await;
            guard
                .get(&id)
                .map(|req| req.diagnostics.clone())
                .unwrap_or_default()
        };
        entry.diagnostics = Some(diagnostics);
    }

    if entry.is_ready() {
        triggers::evaluate_and_maybe_fire(raw_id, ctx, debounce, request_buf, breaker).await;
    }
}

async fn handle_event(
    event: ProxyEvent,
    ctx: &FleetCtx,
    debounce: &mut HashMap<(String, i64), DebounceLedger>,
    request_buf: &mut HashMap<u64, RequestBuffer>,
    breaker: &mut CircuitBreaker,
) {
    match event {
        ProxyEvent::ResponseComplete {
            id, stop_reason, ..
        } => {
            handle_response_complete(id, stop_reason, ctx, debounce, request_buf, breaker).await;
        }

        ProxyEvent::DiagnosticReady {
            id,
            count,
            accumulated_text,
        } => {
            handle_diagnostic_ready(
                DiagnosticReadyEvent {
                    id,
                    count,
                    accumulated_text,
                },
                ctx,
                debounce,
                request_buf,
                breaker,
            )
            .await;
        }

        ProxyEvent::UserPromptReady {
            request_id,
            session_id,
            user_text,
        } => {
            let raw_id = request_id.as_u64();
            let stripped = strip_harness_scaffold(&user_text);
            if !stripped.is_empty() {
                triggers::evaluate_user_prompt_triggers(
                    raw_id, session_id, stripped, ctx, debounce, breaker,
                )
                .await;
            }
        }

        _ => {}
    }
}
