use crate::store::persistence::steerer_repo::SteererPersist as _;
use std::collections::HashMap;

use tracing::warn;

use super::context::{EvalInputs, EvalTarget, SteererSelection};
use super::judge::{
    JudgeDeps, JudgeTurnCloseNowOutcome, run_turn_close_window, turn_close_fleet_ctx,
};
use super::steerer_eval::Abstention;
use crate::store::persistence::operator_turns::QuiescentTurnRow;
use crate::store::persistence::steering::EnrolmentRow;

fn not_judged(reason: Abstention) -> JudgeTurnCloseNowOutcome {
    tracing::debug!(abstention = reason.code(), "turn-close judge: not judged");
    JudgeTurnCloseNowOutcome {
        fired: None,
        llm_call_failed: false,
        llm_call_refused: false,
    }
}

async fn enrolment_for(
    deps: &JudgeDeps<'_>,
    row: &QuiescentTurnRow,
) -> Result<Option<EnrolmentRow>, ()> {
    match deps
        .persist
        .steerer()
        .list_active_enrolments_for_session(row.session_uuid)
        .await
    {
        Ok(rows) => Ok(rows.into_iter().find(|e| e.steerer_id == row.steerer_id)),
        Err(e) => {
            warn!(
                session_id = %row.session_uuid,
                steerer_id = row.steerer_id,
                error = %e,
                "idle judge: enrolment lookup failed — skipping"
            );
            Err(())
        }
    }
}

pub(crate) async fn judge_idle_turn(
    deps: JudgeDeps<'_>,
    row: &QuiescentTurnRow,
    live_text: String,
) -> JudgeTurnCloseNowOutcome {
    let registry = deps.steerer_registry.load_full();
    let Some(steerer) = registry.enabled().find(|s| s.id == row.steerer_id) else {
        return not_judged(Abstention::NoTurnClosedSteerer);
    };
    let Ok(enrolment) = enrolment_for(&deps, row).await else {
        return not_judged(Abstention::EnrolmentLookupFailed);
    };
    let Some(enrolment) = enrolment else {
        return not_judged(Abstention::NoTurnClosedSteerer);
    };
    let eval_inputs = EvalInputs {
        eval_text: live_text,
        trigger_point: "quiescence",
        turn_origin: "primary".to_owned(),
        judged_turn_id: Some(row.turn_id),
        evaluation_turn_id: Some(row.turn_id),
        question: row.question.clone(),
        ..Default::default()
    };
    let target = EvalTarget {
        raw_id: u64::try_from(row.end_request_id).unwrap_or(0),
        trigger_request_id: Some(row.end_request_id),
        session_id: row.session_uuid.to_string(),
    };
    let selection = SteererSelection {
        steerers: vec![steerer],
        enrolment_by_steerer_id: Some(HashMap::from([(steerer.id, enrolment)])),
    };
    let ctx = turn_close_fleet_ctx(&deps);
    run_turn_close_window(&ctx, &target, &eval_inputs, &selection).await
}
