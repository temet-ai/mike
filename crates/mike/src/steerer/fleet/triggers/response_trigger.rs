use std::collections::HashMap;
use std::time::Instant;

use tracing::{debug, warn};

use super::super::context::{
    DebounceLedger, EvalInputs, EvalTarget, FleetCtx, RequestBuffer, SteererSelection,
};
use super::{
    fetch_anchor, is_work_lane, resolve_trigger_steerers, response_trigger_gate,
    run_loop_and_record_breaker,
};
use crate::circuit_breaker::CircuitBreaker;
use crate::steerer::gate::strip_harness_scaffold;
use crate::steerer::verifier::summarise_tool_calls;
use crate::store::models::{Diagnostic, RequestId};
use crate::wire::machine_text::{self, MachineTextKind};

fn log_operator_ask_lookup_failed(
    target: &EvalTarget,
    e: &crate::store::persistence::PersistError,
) {
    warn!(
        request_id = target.raw_id,
        session_id = %target.session_id,
        error = %e,
        "steerer fleet: operator-turn ask lookup failed — judge sees no ask"
    );
}

pub(in crate::steerer::fleet) async fn resolve_operator_ask_with<F, Fut>(
    human_prompt: &str,
    open_turn_prompt: F,
) -> Result<String, crate::store::persistence::PersistError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<
            Output = Result<Option<String>, crate::store::persistence::PersistError>,
        >,
{
    if !human_prompt.is_empty() {
        return Ok(strip_harness_scaffold(human_prompt));
    }
    Ok(open_turn_prompt().await?.unwrap_or_default())
}

async fn resolve_operator_ask(
    ctx: &FleetCtx,
    session_uuid: uuid::Uuid,
    human_prompt: &str,
    target: &EvalTarget,
) -> String {
    let Some(ph) = ctx.persist.as_ref() else {
        return if human_prompt.is_empty() {
            String::new()
        } else {
            strip_harness_scaffold(human_prompt)
        };
    };
    match resolve_operator_ask_with(human_prompt, || ph.get_open_turn_prompt(session_uuid)).await {
        Ok(ask) => ask,
        Err(e) => {
            log_operator_ask_lookup_failed(target, &e);
            String::new()
        }
    }
}

struct ReadyBuffer {
    target: EvalTarget,
    session_uuid: uuid::Uuid,
    last_user_message: String,
    findings: Vec<Diagnostic>,
    accumulated_text: String,
    stop_reason: Option<String>,
    text_non_empty: bool,
}

fn log_no_session_id(raw_id: u64) {
    debug!(
        request_id = raw_id,
        "steerer fleet: no session_id — skipping"
    );
}

fn log_session_id_not_uuid(raw_id: u64, session_id: &str) {
    debug!(
        request_id = raw_id,
        session_id, "steerer fleet: session_id is not a UUID — skipping"
    );
}

fn decode_ready_buffer(raw_id: u64, buf: RequestBuffer) -> Option<ReadyBuffer> {
    let Some(session_id) = buf.session_id else {
        log_no_session_id(raw_id);
        return None;
    };
    let Ok(session_uuid) = session_id.parse::<uuid::Uuid>() else {
        log_session_id_not_uuid(raw_id, &session_id);
        return None;
    };
    let accumulated_text = buf.accumulated_text.unwrap_or_default();
    let text_non_empty = !accumulated_text.is_empty();
    Some(ReadyBuffer {
        target: EvalTarget::from_raw(session_id, raw_id),
        session_uuid,
        last_user_message: buf.last_user_message.unwrap_or_default(),
        findings: buf.diagnostics.unwrap_or_default(),
        accumulated_text,
        stop_reason: buf.stop_reason,
        text_non_empty,
    })
}

struct ResponseContext {
    tool_calls_summary: String,
    turn_origin: String,
    subagent_persona: Option<String>,
    human_prompt: String,
}

fn log_sidecar_lane_skipped(raw_id: u64, session_id: &str, turn_origin: &str) {
    debug!(
        request_id = raw_id,
        session_id, turn_origin, "steerer fleet: sidecar/probe lane — skipping evaluation"
    );
}

async fn gather_response_context(
    ctx: &FleetCtx,
    raw_id: u64,
    session_id: &str,
) -> Option<ResponseContext> {
    let request_id = RequestId::from_raw(raw_id);
    let (tool_calls_summary, turn_origin, subagent_persona) = {
        let guard = ctx.store.read().await;
        match guard.get(&request_id) {
            Some(req) => (
                summarise_tool_calls(&req.tool_calls),
                req.turn_origin.clone(),
                req.subagent_persona.clone(),
            ),
            None => (String::new(), String::new(), None),
        }
    };

    if !is_work_lane(&turn_origin) {
        log_sidecar_lane_skipped(raw_id, session_id, &turn_origin);
        return None;
    }

    let human_prompt = {
        let guard = ctx.store.read().await;
        guard
            .get(&request_id)
            .map(|req| req.human_prompt.clone())
            .unwrap_or_default()
    };

    Some(ResponseContext {
        tool_calls_summary,
        turn_origin,
        subagent_persona,
        human_prompt,
    })
}

fn log_no_eligible_steerers(raw_id: u64, session_id: &str) {
    debug!(
        request_id = raw_id,
        session_id, "steerer fleet: no enabled enrolled steerers for response trigger — skipping"
    );
}

fn log_breaker_open_response(raw_id: u64, session_id: &str) {
    debug!(
        request_id = raw_id,
        session_id, "steerer fleet: circuit breaker open — skipping evaluation"
    );
}

fn response_evaluation_gates_pass(
    raw_id: u64,
    session_id: &str,
    selection: &SteererSelection<'_>,
    breaker: &CircuitBreaker,
) -> bool {
    if selection.steerers.is_empty() {
        log_no_eligible_steerers(raw_id, session_id);
        return false;
    }
    if breaker.should_skip(Instant::now()) {
        log_breaker_open_response(raw_id, session_id);
        return false;
    }
    true
}

fn log_machine_authored_ask(raw_id: u64, session_id: &str, kind: MachineTextKind) {
    debug!(
        request_id = raw_id,
        session_id,
        ?kind,
        "steerer fleet: machine-authored ask — skipping evaluation"
    );
}

pub(crate) async fn evaluate_and_maybe_fire(
    raw_id: u64,
    ctx: &FleetCtx,
    debounce: &mut HashMap<(String, i64), DebounceLedger>,
    request_buf: &mut HashMap<u64, RequestBuffer>,
    breaker: &mut CircuitBreaker,
) {
    let Some(buf) = request_buf.remove(&raw_id) else {
        return;
    };
    let Some(ready) = decode_ready_buffer(raw_id, buf) else {
        return;
    };
    let ReadyBuffer {
        target,
        session_uuid,
        last_user_message,
        findings,
        accumulated_text,
        stop_reason,
        text_non_empty,
    } = ready;
    let session_id = target.session_id.clone();

    let anchor = fetch_anchor(ctx, session_uuid, &target).await;

    let Some(response_ctx) = gather_response_context(ctx, raw_id, &session_id).await else {
        return;
    };
    let ResponseContext {
        tool_calls_summary,
        turn_origin,
        subagent_persona,
        human_prompt,
    } = response_ctx;

    let registry = ctx.steerer_registry.load_full();
    let Some(selection) =
        resolve_trigger_steerers(ctx, &registry, session_uuid, &target, "response", |s| {
            response_trigger_gate(&s.trigger_point, stop_reason.as_deref(), text_non_empty)
        })
        .await
    else {
        return;
    };

    if !response_evaluation_gates_pass(raw_id, &session_id, &selection, breaker) {
        return;
    }
    if ctx.shutdown.is_cancelled() {
        return;
    }

    let operator_ask = resolve_operator_ask(ctx, session_uuid, &human_prompt, &target).await;

    if let Some(kind) = machine_text::classify(&operator_ask) {
        log_machine_authored_ask(raw_id, &session_id, kind);
        return;
    }

    let eval_inputs = EvalInputs {
        eval_text: accumulated_text,
        operator_ask,
        last_user_message,
        tool_calls_summary,
        diagnostics: findings,
        anchor,
        trigger_point: "response",
        turn_origin,
        subagent_persona,
        judged_turn_id: None,
        evaluation_turn_id: None,
        question: None,
    };

    run_loop_and_record_breaker(ctx, &target, &eval_inputs, &selection, debounce, breaker).await;
}
