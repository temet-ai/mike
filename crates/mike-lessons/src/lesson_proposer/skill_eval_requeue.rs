use crate::store::persistence::lessons_repo::LessonsPersist as _;

use super::EVALUATOR_VERSION;
use crate::store::persistence::{PersistError, PersistHandle};

pub(super) async fn requeue_stale(persist: &PersistHandle) -> i64 {
    let requeued = persist
        .lessons()
        .requeue_skill_drafts_before_evaluator(EVALUATOR_VERSION)
        .await
        .unwrap_or_else(|e| {
            log_requeue_failed(&e);
            0
        });
    log_requeued(requeued);
    requeued
}

fn log_requeued(requeued: i64) {
    if requeued > 0 {
        tracing::info!(
            requeued,
            evaluator_version = EVALUATOR_VERSION,
            "skill eval: drafts rejected by an older evaluator put back in the queue"
        );
    }
}

fn log_requeue_failed(error: &PersistError) {
    tracing::warn!(%error, "skill eval: older rejections not requeued");
}
