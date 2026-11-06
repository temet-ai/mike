use super::spawn::UpdaterLoopCtx;
use crate::store::models::RequestId;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::harvest_runs::NewHarvestRun;

pub(super) async fn run_harvester_hook(
    ctx: &UpdaterLoopCtx<'_>,
    persist: &PersistHandle,
    text_for_analysis: &str,
) {
    let request_id = ctx.block_target.request_id;
    let (project_path, session_uuid, session_db_id) = resolve_harvest_scope(ctx).await;
    match persist
        .resolve_lesson_project(&session_uuid, &project_path)
        .await
    {
        Ok(project_id) => {
            harvest_and_log(
                ctx,
                persist,
                text_for_analysis,
                project_id,
                session_db_id,
                &session_uuid,
            )
            .await;
        }
        Err(e) => {
            tracing::warn!(
                request_id = %request_id,
                project_path = %project_path,
                error = %e,
                "harvester: failed to resolve project_id — skipping harvest"
            );
        }
    }
}

async fn resolve_harvest_scope(ctx: &UpdaterLoopCtx<'_>) -> (String, String, Option<i64>) {
    let request_id = ctx.block_target.request_id;
    let s = ctx.block_target.store.read().await;
    let project_path = s
        .get(&request_id)
        .and_then(|r| r.project.clone())
        .unwrap_or_else(|| crate::store::persistence::SENTINEL_PROJECT_PATH.to_string());
    let session_uuid = s
        .get(&request_id)
        .map(|r| r.session_id.clone())
        .unwrap_or_default();
    let session_db_id = s.session_id(&session_uuid);
    (project_path, session_uuid, session_db_id)
}

async fn harvest_and_log(
    ctx: &UpdaterLoopCtx<'_>,
    persist: &PersistHandle,
    text_for_analysis: &str,
    project_id: i64,
    session_db_id: Option<i64>,
    session_uuid: &str,
) {
    let harvest_ctx = crate::memory_harvester::HarvestContext {
        project_id,
        session_id: session_db_id,
        request_id: Some(ctx.request_id_i64),
        source_turn_id: None,
    };
    let result = crate::memory_harvester::harvest_from_response(
        text_for_analysis,
        &harvest_ctx,
        persist,
        ctx.harvester_cfg,
    )
    .await;
    if result.tags_found > 0 {
        log_harvest_tags_found(ctx.block_target.request_id, session_uuid, &result);
        record_harvest_run(ctx, persist, project_id, session_db_id, &result).await;
    }
}

async fn record_harvest_run(
    ctx: &UpdaterLoopCtx<'_>,
    persist: &PersistHandle,
    project_id: i64,
    session_db_id: Option<i64>,
    result: &crate::memory_harvester::HarvestResult,
) {
    let run = NewHarvestRun {
        project_id,
        session_id: session_db_id,
        request_id: ctx.request_id_i64,
        tags_found: result.tags_found,
        lessons_written: result.lessons_written,
        lessons_skipped: result.lessons_skipped,
        lessons_truncated: result.lessons_truncated,
    };
    if let Err(e) = persist.record_harvest_run(run).await {
        tracing::warn!(
            request_id = %ctx.block_target.request_id,
            error = %e,
            "harvester: failed to record harvest run"
        );
    }
}

fn log_harvest_tags_found(
    request_id: RequestId,
    session_uuid: &str,
    result: &crate::memory_harvester::HarvestResult,
) {
    tracing::info!(
        request_id = %request_id,
        session_id = %session_uuid,
        tags_found = result.tags_found,
        lessons_written = result.lessons_written,
        lessons_skipped = result.lessons_skipped,
        lessons_truncated = result.lessons_truncated,
        "harvester: processed mem:update tags"
    );
}
