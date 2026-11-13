use super::ProposerDeps;
use crate::store::persistence::lessons::maintenance_budget::{MaintenancePass, RefusalReason};
use crate::store::persistence::lessons_repo::LessonsPersist as _;

pub(super) enum Reservation {
    Reserved,
    Refused,
    Failed,
}

pub(super) async fn reserve(deps: &ProposerDeps<'_>, pass: MaintenancePass) -> Reservation {
    if deps.llm.budget().calls_now == 0 {
        note_refusal(deps, pass, RefusalReason::ProviderLimit).await;
        return Reservation::Refused;
    }
    let cap = deps.settings.maintenance_max_per_day;
    match deps
        .persist
        .lessons()
        .reserve_lesson_maintenance_call(cap)
        .await
    {
        Ok(true) => Reservation::Reserved,
        Ok(false) => {
            note_refusal(deps, pass, RefusalReason::DailyBudget).await;
            Reservation::Refused
        }
        Err(error) => {
            tracing::warn!(%error, pass = pass.code(), "lesson_proposer: maintenance budget reservation failed");
            Reservation::Failed
        }
    }
}

pub(super) async fn note_refusal(
    deps: &ProposerDeps<'_>,
    pass: MaintenancePass,
    reason: RefusalReason,
) {
    let cap = deps.settings.maintenance_max_per_day;
    match deps
        .persist
        .lessons()
        .record_maintenance_refusal(pass, reason, cap)
        .await
    {
        Ok(true) => log_refused(pass, reason, cap),
        Ok(false) => {}
        Err(error) => {
            tracing::warn!(%error, pass = pass.code(), "lesson_proposer: maintenance refusal could not be recorded");
        }
    }
}

fn log_refused(pass: MaintenancePass, reason: RefusalReason, cap: i64) {
    tracing::info!(
        pass = pass.code(),
        reason = reason.code(),
        daily_cap = cap,
        "lesson_proposer: maintenance pass refused a model call; setting judge_lessons.maintenance_max_per_day"
    );
}
