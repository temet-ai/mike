use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Default)]
struct NightlyTally {
    total_promoted: u64,
    total_trimmed: u64,
    relations_confirmed: u64,
    relations_applied: u64,
    shape_released: u64,
    any_error: bool,
}

const SHAPE_HOLD_DAYS: i32 = 14;

pub(super) async fn run_nightly_consolidation(
    persist: &PersistHandle,
    max_lessons_per_project: i64,
    trigger: &str,
) {
    log_nightly_starting();

    let Some(run_id) = open_nightly_run(persist, trigger).await else {
        return;
    };

    let Some(project_ids) = discover_daily_projects(persist, run_id).await else {
        return;
    };

    let mut tally = NightlyTally::default();
    for &project_id in &project_ids {
        promote_and_trim_project(
            persist,
            project_id,
            max_lessons_per_project,
            run_id,
            &mut tally,
        )
        .await;
    }

    apply_lesson_relations(persist, &mut tally).await;

    log_nightly_complete(project_ids.len(), &tally);
    finish_nightly_run(persist, run_id, tally.any_error).await;
}

async fn apply_lesson_relations(persist: &PersistHandle, tally: &mut NightlyTally) {
    match persist.confirm_judge_only_write_time_relations().await {
        Ok(n) => tally.relations_confirmed += n,
        Err(e) => {
            log_relations_failed("confirm_judge_only_write_time_relations", &e);
            tally.any_error = true;
        }
    }
    match persist.apply_confirmed_lesson_relations().await {
        Ok(n) => tally.relations_applied += n,
        Err(e) => {
            log_relations_failed("apply_confirmed_lesson_relations", &e);
            tally.any_error = true;
        }
    }
    match persist.release_stale_shape_findings(SHAPE_HOLD_DAYS).await {
        Ok(n) => tally.shape_released += n,
        Err(e) => {
            log_relations_failed("release_stale_shape_findings", &e);
            tally.any_error = true;
        }
    }
}

fn log_relations_failed(step: &str, e: &PersistError) {
    tracing::warn!(error = %e, step, "lesson_worker: lesson relation step failed");
}

fn log_nightly_starting() {
    tracing::info!("lesson_worker: nightly consolidation starting");
}

async fn open_nightly_run(persist: &PersistHandle, trigger: &str) -> Option<i64> {
    match persist.start_consolidation_run("nightly", trigger).await {
        Ok(id) => Some(id),
        Err(e) => {
            log_open_run_failed(&e);
            None
        }
    }
}

fn log_open_run_failed(e: &PersistError) {
    tracing::warn!(
        error = %e,
        "lesson_worker: failed to open the nightly consolidation run — skipping this pass, will retry next scheduled run"
    );
}

async fn discover_daily_projects(persist: &PersistHandle, run_id: i64) -> Option<Vec<i64>> {
    match persist.discover_daily_projects().await {
        Ok(ids) => Some(ids),
        Err(e) => {
            log_discover_failed(&e);
            close_failed_nightly_run(persist, run_id, &e).await;
            None
        }
    }
}

fn log_discover_failed(e: &PersistError) {
    tracing::warn!(error = %e, "lesson_worker: failed to discover daily-stage projects");
}

async fn close_failed_nightly_run(persist: &PersistHandle, run_id: i64, e: &PersistError) {
    if let Err(finish_err) = persist
        .finish_consolidation_run(run_id, "failed", Some(&e.to_string()))
        .await
    {
        log_close_run_failed(run_id, &finish_err);
    }
}

fn log_close_run_failed(run_id: i64, e: &PersistError) {
    tracing::warn!(
        error = %e,
        run_id,
        "lesson_worker: failed to close the nightly consolidation run"
    );
}

async fn promote_and_trim_project(
    persist: &PersistHandle,
    project_id: i64,
    max_lessons_per_project: i64,
    run_id: i64,
    tally: &mut NightlyTally,
) {
    match persist.promote_daily_to_long_term(project_id, run_id).await {
        Ok(n) => tally.total_promoted += n,
        Err(e) => {
            log_promote_failed(project_id, &e);
            tally.any_error = true;
        }
    }

    match persist
        .trim_lessons_to_max(project_id, max_lessons_per_project, run_id)
        .await
    {
        Ok(n) => tally.total_trimmed += n,
        Err(e) => {
            log_trim_failed(project_id, &e);
            tally.any_error = true;
        }
    }
}

fn log_promote_failed(project_id: i64, e: &PersistError) {
    tracing::warn!(error = %e, project_id, "lesson_worker: promote_daily_to_long_term failed");
}

fn log_trim_failed(project_id: i64, e: &PersistError) {
    tracing::warn!(error = %e, project_id, "lesson_worker: trim_lessons_to_max failed");
}

fn log_nightly_complete(project_count: usize, tally: &NightlyTally) {
    tracing::info!(
        project_count,
        total_promoted = tally.total_promoted,
        total_trimmed = tally.total_trimmed,
        relations_confirmed = tally.relations_confirmed,
        relations_applied = tally.relations_applied,
        shape_released = tally.shape_released,
        "lesson_worker: nightly consolidation complete"
    );
}

async fn finish_nightly_run(persist: &PersistHandle, run_id: i64, any_error: bool) {
    let status = if any_error { "partial" } else { "completed" };
    if let Err(e) = persist.finish_consolidation_run(run_id, status, None).await {
        log_close_run_failed(run_id, &e);
    }
}
