use tracing::{debug, warn};
use uuid::Uuid;

use super::context::{EvalInputs, EvalTarget, FleetCtx};
use super::input_budget::{VerifierInputBudget, project_prior_fires};
use super::verifier_judge::{VerifierJudgeContext, build_verifier_judge_context};
use crate::steerer::arming::{ArmingState, derive_arming_state, has_explicit_done_claim};
use crate::steerer::registry::Steerer;
use crate::steerer::verifier::build_prior_fires_digest;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::steering::ArmingHistoryRow;

async fn fetch_arming_history_or_default(
    ph: &PersistHandle,
    session_db_id: i64,
) -> ArmingHistoryRow {
    match ph.fetch_arming_history(session_db_id).await {
        Ok(h) => h,
        Err(e) => {
            warn!(
                session_db_id,
                err = %e,
                "steerer fleet: arming-history fetch failed — failing open to Calm"
            );
            ArmingHistoryRow {
                closed_turn_count: i64::MAX,
                recent_outcomes_newest_first: Vec::new(),
            }
        }
    }
}

async fn fetch_prior_fires_digest(
    ph: &PersistHandle,
    session_db_id: i64,
    steerer_id: i64,
) -> String {
    let prior_fires = match ph
        .fetch_prior_fires_for_session(session_db_id, steerer_id, 10)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            warn!(
                session_db_id,
                steerer_id,
                err = %e,
                "steerer fleet: prior-fires fetch failed — judge sees no fire memory"
            );
            Vec::new()
        }
    };
    build_prior_fires_digest(&prior_fires)
}

async fn evaluate_verifier_arming(
    ph: &PersistHandle,
    session_db_id: i64,
    steerer: &Steerer,
    eval_text_for_done_claim: &str,
) -> Option<String> {
    let history = fetch_arming_history_or_default(ph, session_db_id).await;
    let recent: Vec<&str> = history
        .recent_outcomes_newest_first
        .iter()
        .map(String::as_str)
        .collect();
    let has_done_claim = has_explicit_done_claim(eval_text_for_done_claim);
    let state = derive_arming_state(
        &recent,
        steerer.params.arming_window_turns as usize,
        history.closed_turn_count,
        steerer.params.suppress_first_n_turns as i64,
        has_done_claim,
    );
    if state == ArmingState::Suppressed {
        return None;
    }
    Some(fetch_prior_fires_digest(ph, session_db_id, steerer.id).await)
}

pub(super) async fn verifier_arming_check(
    ctx: &FleetCtx,
    steerer: &Steerer,
    eval_inputs: &EvalInputs,
    session_uuid: Option<Uuid>,
    verifier_context: &mut Option<VerifierJudgeContext>,
    target: &EvalTarget,
) -> bool {
    if steerer.kind != "verifier" {
        return false;
    }
    let session_db_id = match (ctx.persist.as_ref(), session_uuid) {
        (Some(ph), Some(uuid)) => ph
            .resolve_session_db_id(&uuid.to_string())
            .await
            .ok()
            .flatten(),
        _ => None,
    };
    let (Some(ph), Some(db_id)) = (ctx.persist.as_ref(), session_db_id) else {
        return false;
    };
    match evaluate_verifier_arming(ph, db_id, steerer, &eval_inputs.eval_text).await {
        None => {
            debug!(
                request_id = target.raw_id,
                session_id = %target.session_id,
                steerer_id = steerer.id,
                "steerer fleet: verifier suppressed by risk-based arming — no LLM call"
            );
            true
        }
        Some(prior_fires_digest) => {
            if verifier_context.is_none()
                && let Some(uuid) = session_uuid
            {
                let budget = VerifierInputBudget::for_steerer_kind(&ctx.settings, &steerer.kind);
                let mut context = build_verifier_judge_context(
                    ph,
                    uuid,
                    Some(db_id),
                    eval_inputs.judged_turn_id,
                    budget,
                )
                .await;
                context.prior_fires_digest = project_prior_fires(prior_fires_digest, budget);
                *verifier_context = Some(context);
            }
            false
        }
    }
}

pub(super) async fn ensure_verifier_context_fallback(
    ctx: &FleetCtx,
    steerer: &Steerer,
    eval_inputs: &EvalInputs,
    session_uuid: Option<Uuid>,
    verifier_context: &mut Option<VerifierJudgeContext>,
) {
    if steerer.kind != "verifier" || verifier_context.is_some() {
        return;
    }
    let (Some(ph), Some(uuid)) = (ctx.persist.as_ref(), session_uuid) else {
        return;
    };
    let budget = VerifierInputBudget::for_steerer_kind(&ctx.settings, &steerer.kind);
    *verifier_context = Some(
        build_verifier_judge_context(ph, uuid, None, eval_inputs.judged_turn_id, budget).await,
    );
}
