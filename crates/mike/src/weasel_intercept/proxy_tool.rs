use bytes::Bytes;
use tokio::sync::mpsc;

use super::followup::spawn_stitched_tee;
use super::stitch;
use super::support::{LaneIdentity, make_correction_intercepted_request, proxy_tool_result_prompt};
use crate::state::AppState;
use crate::store::RequestStore;
use crate::store::models::RequestId;

pub(super) struct ProxyToolLoopCtx {
    pub(super) state: AppState,
    pub(super) upstream_url: String,
    pub(super) forward_headers: reqwest::header::HeaderMap,
    pub(super) original_body: Bytes,
    pub(super) original_request_id: RequestId,
    pub(super) session_id: String,
    pub(super) project_path: String,
}

pub(super) async fn run_proxy_tool_loop(
    ctx: ProxyToolLoopCtx,
    (call, prelude): (
        crate::proxy_tools::answer::ProxyToolCall,
        crate::proxy_tools::answer::AssistantPrelude,
    ),
    body_tx: mpsc::Sender<Result<Bytes, std::io::Error>>,
    next_index: u64,
) {
    let ProxyToolLoopCtx {
        state,
        upstream_url,
        forward_headers,
        original_body,
        original_request_id,
        session_id,
        project_path,
    } = ctx;

    log_proxy_tool_call_intercepted(original_request_id, &call);

    let tool_result =
        crate::proxy_tools::answer::answer_tool_call(&state, &call, &project_path).await;

    let followup_body = match crate::proxy_tools::answer::build_followup_body(
        &original_body,
        &prelude,
        &call,
        &tool_result,
    ) {
        Ok(b) => b,
        Err(e) => {
            warn_followup_body_build_failed(original_request_id, &e);
            end_with_error(&body_tx).await;
            return;
        }
    };

    let created_at = chrono::Utc::now();
    let lane = LaneIdentity::of_request(&state, original_request_id).await;
    let mut minimal = make_correction_intercepted_request(
        RequestId::next(),
        session_id,
        followup_body.clone(),
        proxy_tool_result_prompt(&call.name),
        created_at,
        lane,
    );

    let followup_request_id =
        RequestStore::insert_request_stub_to_db(&state.store, &minimal, false).await;
    minimal.id = followup_request_id;
    {
        let mut store = state.store.write().await;
        store.insert(minimal);
        store.update(&original_request_id, |req| {
            req.intercepted = true;
            req.correction_request_id = Some(followup_request_id.as_u64());
        });
    }
    if let Some(persist) = state.persist() {
        persist
            .mark_intercepted(
                original_request_id.as_u64() as i64,
                followup_request_id.as_u64() as i64,
            )
            .await;
    }

    let upstream_resp = match state
        .client
        .post(upstream_url)
        .headers(forward_headers)
        .body(followup_body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            warn_followup_upstream_failed(original_request_id, &e);
            end_with_error(&body_tx).await;
            return;
        }
    };

    spawn_stitched_tee(
        &state,
        upstream_resp,
        body_tx,
        followup_request_id,
        next_index,
    );
}

async fn end_with_error(body_tx: &mpsc::Sender<Result<Bytes, std::io::Error>>) {
    let _ = body_tx
        .send(Ok(stitch::error_frame(
            "the mike proxy answered a call to one of its own tools and could not ask upstream for the rest of the reply",
        )))
        .await;
}

fn log_proxy_tool_call_intercepted(
    request_id: RequestId,
    call: &crate::proxy_tools::answer::ProxyToolCall,
) {
    tracing::info!(
        request_id = %request_id,
        tool = %call.name,
        tool_use_id = %call.id,
        "proxy tool call intercepted — answering inside the proxy"
    );
}

fn warn_followup_body_build_failed(request_id: RequestId, e: &impl std::fmt::Display) {
    tracing::warn!(
        request_id = %request_id,
        error = %e,
        "proxy tool follow-up body build failed — ending the reply with an error"
    );
}

fn warn_followup_upstream_failed(request_id: RequestId, e: &impl std::fmt::Display) {
    tracing::warn!(
        request_id = %request_id,
        error = %e,
        "proxy tool follow-up upstream request failed — ending the reply with an error"
    );
}
