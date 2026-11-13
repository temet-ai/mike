use super::spawn::UpdaterLoopCtx;

pub struct ResponseScope {
    pub project_path: String,
    pub session_uuid: String,
}

pub async fn read_response_scope(ctx: &UpdaterLoopCtx<'_>) -> ResponseScope {
    let request_id = ctx.block_target.request_id;
    let s = ctx.block_target.store.read().await;
    let request = s.get(&request_id);
    ResponseScope {
        project_path: request
            .and_then(|r| r.project.clone())
            .unwrap_or_else(|| crate::store::persistence::SENTINEL_PROJECT_PATH.to_string()),
        session_uuid: request.map(|r| r.session_id.clone()).unwrap_or_default(),
    }
}
