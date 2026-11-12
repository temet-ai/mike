use std::collections::HashMap;
use std::time::Instant;

use tracing::debug;

use super::super::ask_ledger::{append_ask_ledger_entry_for_turn, reauthor_intake_criteria};
use super::super::context::{DebounceLedger, EvalInputs, EvalTarget, FleetCtx};
use super::{fetch_anchor, is_work_lane, resolve_trigger_steerers, run_loop_and_record_breaker};
use crate::circuit_breaker::CircuitBreaker;
use crate::store::models::RequestId;
use crate::wire::genuine::is_genuine_text;
use crate::wire::machine_text::{self, MachineTextKind};

fn log_user_prompt_session_id_not_uuid(raw_id: u64, session_id: &str) {
    debug!(
        request_id = raw_id,
        session_id, "steerer fleet: user_prompt session_id is not a UUID — skipping"
    );
}

fn log_user_prompt_sidecar_lane_skipped(raw_id: u64, session_id: &str, turn_origin: &str) {
    debug!(
        request_id = raw_id,
        session_id,
        turn_origin,
        "steerer fleet: sidecar/probe lane — skipping user_prompt evaluation"
    );
}

fn log_machine_authored_user_prompt(raw_id: u64, session_id: &str, kind: MachineTextKind) {
    debug!(
        request_id = raw_id,
        session_id,
        ?kind,
        "steerer fleet: machine-authored user prompt — skipping evaluation"
    );
}

fn log_breaker_open_user_prompt(raw_id: u64, session_id: &str) {
    debug!(
        request_id = raw_id,
        session_id, "steerer fleet: circuit breaker open — skipping user_prompt evaluation"
    );
}

async fn gather_user_prompt_context(
    ctx: &FleetCtx,
    raw_id: u64,
    session_id: &str,
) -> Option<(String, Option<String>)> {
    let request_id = RequestId::from_raw(raw_id);
    let (turn_origin, subagent_persona) = {
        let guard = ctx.store.read().await;
        match guard.get(&request_id) {
            Some(req) => (req.turn_origin.clone(), req.subagent_persona.clone()),
            None => (String::new(), None),
        }
    };

    if !is_work_lane(&turn_origin) {
        log_user_prompt_sidecar_lane_skipped(raw_id, session_id, &turn_origin);
        return None;
    }

    Some((turn_origin, subagent_persona))
}

async fn accrete_ask_ledger(
    ctx: &FleetCtx,
    session_uuid: uuid::Uuid,
    raw_id: u64,
    stripped_user_text: &str,
) {
    let Some(ph) = ctx.persist.as_ref() else {
        return;
    };
    let raw_id = i64::try_from(raw_id).unwrap_or(i64::MAX);
    append_ask_ledger_entry_for_turn(ph, session_uuid, raw_id, stripped_user_text).await;
    reauthor_intake_criteria(ctx, ph, session_uuid).await;
}

pub(crate) async fn evaluate_user_prompt_triggers(
    raw_id: u64,
    session_id: String,
    stripped_user_text: String,
    ctx: &FleetCtx,
    debounce: &mut HashMap<(String, i64), DebounceLedger>,
    breaker: &mut CircuitBreaker,
) {
    let Ok(session_uuid) = session_id.parse::<uuid::Uuid>() else {
        log_user_prompt_session_id_not_uuid(raw_id, &session_id);
        return;
    };

    let target = EvalTarget::from_raw(session_id.clone(), raw_id);
    let anchor = fetch_anchor(ctx, session_uuid, &target).await;

    let Some((turn_origin, subagent_persona)) =
        gather_user_prompt_context(ctx, raw_id, &session_id).await
    else {
        return;
    };

    if let Some(kind) = machine_text::classify(&stripped_user_text) {
        log_machine_authored_user_prompt(raw_id, &session_id, kind);
        return;
    }
    if !is_genuine_text(&stripped_user_text) {
        debug!(
            request_id = raw_id,
            session_id, "steerer fleet: non-genuine user prompt — skipping evaluation"
        );
        return;
    }

    accrete_ask_ledger(ctx, session_uuid, raw_id, &stripped_user_text).await;

    let registry = ctx.steerer_registry.load_full();

    let Some(selection) =
        resolve_trigger_steerers(ctx, &registry, session_uuid, &target, "user_prompt", |s| {
            s.trigger_point == "user_prompt"
        })
        .await
    else {
        return;
    };

    if selection.steerers.is_empty() {
        return;
    }

    if breaker.should_skip(Instant::now()) {
        log_breaker_open_user_prompt(raw_id, &session_id);
        return;
    }

    if ctx.shutdown.is_cancelled() {
        return;
    }

    let eval_inputs = EvalInputs {
        eval_text: stripped_user_text.clone(),
        operator_ask: stripped_user_text.clone(),
        last_user_message: stripped_user_text,
        tool_calls_summary: String::new(),
        diagnostics: Vec::new(),
        anchor,
        trigger_point: "user_prompt",
        turn_origin,
        subagent_persona,
        judged_turn_id: None,
        evaluation_turn_id: None,
        question: None,
    };

    run_loop_and_record_breaker(ctx, &target, &eval_inputs, &selection, debounce, breaker).await;
}
