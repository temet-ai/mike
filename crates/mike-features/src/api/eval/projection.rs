use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::eval_labels::EvalLabelRow;
use crate::store::persistence::operator_turns::projection::{ProjectionMode, ProjectionParams};

pub fn projected_turn_ids(labels: &[EvalLabelRow]) -> Vec<i64> {
    let mut ids: Vec<i64> = labels
        .iter()
        .filter(|l| matches!(l.subject_kind.as_str(), "turn" | "session_end"))
        .map(|l| l.operator_turn_id)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

pub async fn project_after_gold(
    state: &AppState,
    persist: &PersistHandle,
    labels: &[EvalLabelRow],
) {
    let overlay = state.settings_overlay.load();
    if overlay.turn_judge_live_bundle_id <= 0 {
        return;
    }
    let ids = projected_turn_ids(labels);
    if ids.is_empty() {
        return;
    }
    let params = ProjectionParams {
        live_bundle_id: overlay.turn_judge_live_bundle_id,
        confidence_threshold: overlay
            .judge_worker_confidence_threshold
            .unwrap_or(state.config().steerer.judge.confidence_threshold),
        mode: ProjectionMode::Apply,
        session_judge_live_bundle_id: overlay.session_judge_live_bundle_id,
    };
    if let Err(e) = persist.project_turn_outcomes(&ids, &params).await {
        tracing::warn!(error = %e, "gold label: projection failed; the next projection applies it");
    }
}
