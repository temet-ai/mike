//! The memory harvester as a response hook: after a response completes, it
//! reads the response's mem:update tags and writes them as lessons.
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::lessons_repo::LessonsPersist as _;

use std::future::Future;
use std::pin::Pin;

use crate::config::memory_harvester::HarvesterConfig;
use crate::store::models::RequestId;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::lessons::harvest_runs::NewHarvestRun;
use crate::store::updater::{CompletedResponse, ResponseHook};

pub(crate) struct HarvesterHook {
    cfg: HarvesterConfig,
}

impl HarvesterHook {
    pub(crate) fn new(cfg: HarvesterConfig) -> Self {
        Self { cfg }
    }
}

impl ResponseHook for HarvesterHook {
    fn on_response<'a>(
        &'a self,
        response: CompletedResponse<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            if self.cfg.enabled {
                run_harvester_hook(&self.cfg, response).await;
            }
        })
    }
}

async fn run_harvester_hook(cfg: &HarvesterConfig, response: CompletedResponse<'_>) {
    let CompletedResponse {
        persist,
        request_id,
        project_path,
        session_uuid,
        text: text_for_analysis,
        ..
    } = response;
    let session_db_id = persist
        .resolve_session_db_id(session_uuid)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                request_id = %request_id,
                error = %e,
                "harvester: failed to resolve the session row"
            );
            None
        });
    match persist
        .knowledge()
        .resolve_lesson_project(session_uuid, project_path)
        .await
    {
        Ok(project_id) => {
            let ids = HarvestIds {
                request_id,
                request_id_i64: response.request_id_i64,
                project_id,
                session_db_id,
            };
            harvest_and_log(cfg, persist, text_for_analysis, &ids, session_uuid).await;
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

struct HarvestIds {
    request_id: RequestId,
    request_id_i64: i64,
    project_id: i64,
    session_db_id: Option<i64>,
}

async fn harvest_and_log(
    cfg: &HarvesterConfig,
    persist: &PersistHandle,
    text_for_analysis: &str,
    ids: &HarvestIds,
    session_uuid: &str,
) {
    let harvest_ctx = super::HarvestContext {
        project_id: ids.project_id,
        session_id: ids.session_db_id,
        request_id: Some(ids.request_id_i64),
        source_turn_id: None,
    };
    let result = super::harvest_from_response(text_for_analysis, &harvest_ctx, persist, cfg).await;
    if result.tags_found > 0 {
        log_harvest_tags_found(ids.request_id, session_uuid, &result);
        record_harvest_run(persist, ids, &result).await;
    }
}

async fn record_harvest_run(
    persist: &PersistHandle,
    ids: &HarvestIds,
    result: &super::HarvestResult,
) {
    let run = NewHarvestRun {
        project_id: ids.project_id,
        session_id: ids.session_db_id,
        request_id: ids.request_id_i64,
        tags_found: result.tags_found,
        lessons_written: result.lessons_written,
        lessons_skipped: result.lessons_skipped,
        lessons_truncated: result.lessons_truncated,
    };
    if let Err(e) = persist.lessons().record_harvest_run(run).await {
        tracing::warn!(
            request_id = %ids.request_id,
            error = %e,
            "harvester: failed to record harvest run"
        );
    }
}

fn log_harvest_tags_found(
    request_id: RequestId,
    session_uuid: &str,
    result: &super::HarvestResult,
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
