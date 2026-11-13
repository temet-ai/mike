use crate::store::persistence::lessons_repo::LessonsPersist as _;
use crate::store::persistence::{PersistError, PersistHandle};

pub async fn run_gc(persist: &PersistHandle) {
    tracing::debug!("lesson_worker: GC starting");

    let Some(run_id) = open_gc_run(persist).await else {
        return;
    };

    match persist.lessons().gc_expired_lessons(run_id).await {
        Ok((archived, deleted)) => handle_gc_success(persist, run_id, archived, deleted).await,
        Err(e) => handle_gc_failure(persist, run_id, &e).await,
    }
}

async fn open_gc_run(persist: &PersistHandle) -> Option<i64> {
    match persist
        .lessons()
        .start_consolidation_run("gc", "schedule")
        .await
    {
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
        "lesson_worker: failed to open the GC run — skipping this pass, will retry next tick"
    );
}

async fn handle_gc_success(persist: &PersistHandle, run_id: i64, archived: u64, deleted: u64) {
    if archived > 0 || deleted > 0 {
        close_nonempty_gc_run(persist, run_id, archived, deleted).await;
    } else {
        discard_empty_gc_run(persist, run_id).await;
    }
}

async fn close_nonempty_gc_run(persist: &PersistHandle, run_id: i64, archived: u64, deleted: u64) {
    log_gc_touched(archived, deleted);
    if let Err(e) = persist
        .lessons()
        .finish_consolidation_run(run_id, "completed", None)
        .await
    {
        log_close_run_failed(run_id, &e);
    }
}

fn log_gc_touched(archived: u64, deleted: u64) {
    tracing::info!(
        archived,
        deleted,
        "lesson_worker: GC archived/deleted expired lessons"
    );
}

fn log_close_run_failed(run_id: i64, e: &PersistError) {
    tracing::warn!(error = %e, run_id, "lesson_worker: failed to close the GC run");
}

async fn discard_empty_gc_run(persist: &PersistHandle, run_id: i64) {
    log_gc_found_nothing();
    if let Err(e) = persist.lessons().delete_consolidation_run(run_id).await {
        log_discard_run_failed(run_id, &e);
    }
}

fn log_gc_found_nothing() {
    tracing::debug!("lesson_worker: GC found no expired lessons");
}

fn log_discard_run_failed(run_id: i64, e: &PersistError) {
    tracing::warn!(
        error = %e,
        run_id,
        "lesson_worker: failed to discard the empty GC run"
    );
}

async fn handle_gc_failure(persist: &PersistHandle, run_id: i64, e: &PersistError) {
    log_gc_failed(e);
    if let Err(finish_err) = persist
        .lessons()
        .finish_consolidation_run(run_id, "failed", Some(&e.to_string()))
        .await
    {
        log_close_failed_run_failed(run_id, &finish_err);
    }
}

fn log_gc_failed(e: &PersistError) {
    tracing::warn!(error = %e, "lesson_worker: GC failed");
}

fn log_close_failed_run_failed(run_id: i64, finish_err: &PersistError) {
    tracing::warn!(
        error = %finish_err,
        run_id,
        "lesson_worker: failed to close the failed GC run"
    );
}
