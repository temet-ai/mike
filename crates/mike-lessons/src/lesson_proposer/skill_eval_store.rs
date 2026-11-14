use serde_json::Value;

use super::progress::Progress;
use super::{EVALUATOR_VERSION, MAX_ATTEMPTS};
use crate::store::persistence::lessons::skill_draft_eval::{EvalDraft, EvalFailure, ShippedPatch};
use crate::store::persistence::lessons_repo::LessonsPersist as _;
use crate::store::persistence::{PersistError, PersistHandle};

pub(super) async fn save_progress(persist: &PersistHandle, draft: &EvalDraft, progress: &Progress) {
    let value = serde_json::to_value(progress).unwrap_or(Value::Null);
    if let Err(e) = persist
        .lessons()
        .save_skill_draft_progress(draft.id, &value)
        .await
    {
        tracing::warn!(error = %e, draft_id = draft.id, "skill eval: progress not saved");
    }
}

pub(super) async fn finish(
    persist: &PersistHandle,
    draft: &EvalDraft,
    accepted: bool,
    eval: &Value,
    patch: Option<ShippedPatch<'_>>,
) {
    if let Err(e) = persist
        .lessons()
        .finish_skill_draft_eval(draft.id, accepted, eval, patch)
        .await
    {
        tracing::warn!(error = %e, draft_id = draft.id, "skill eval: result not stored");
    }
}

pub(super) async fn record_failure(persist: &PersistHandle, draft: &EvalDraft, reason: &str) {
    let recorded = persist
        .lessons()
        .fail_skill_draft_eval(draft.id, reason, MAX_ATTEMPTS, EVALUATOR_VERSION)
        .await;
    match recorded {
        Ok(failure) => log_attempt_failed(draft, reason, failure),
        Err(e) => log_failure_lost(draft, &e),
    }
}

fn log_attempt_failed(draft: &EvalDraft, reason: &str, failure: Option<EvalFailure>) {
    tracing::warn!(
        draft_id = draft.id,
        folder = %draft.folder,
        reason,
        ?failure,
        "skill eval: attempt failed"
    );
}

fn log_failure_lost(draft: &EvalDraft, error: &PersistError) {
    tracing::warn!(%error, draft_id = draft.id, "skill eval: failure not recorded");
}
