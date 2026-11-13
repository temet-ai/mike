use bytes::Bytes;
use tokio::sync::mpsc;

use super::corrector::CorrectionRequest;
use super::followup::spawn_stitched_tee;
use super::stitch;
use super::support::{LaneIdentity, make_correction_intercepted_request, weasel_correction_prompt};
use crate::state::AppState;
use crate::store::RequestStore;
use crate::store::models::RequestId;

pub struct WeaselCorrectionCtx {
    pub state: AppState,
    pub upstream_url: String,
    pub forward_headers: reqwest::header::HeaderMap,
    pub original_body: Bytes,
    pub original_request_id: RequestId,
    pub session_id: String,
    pub correction_prompt: String,
}

pub async fn run_weasel_correction(
    ctx: WeaselCorrectionCtx,
    (trigger_word, weasel_text): (String, String),
    body_tx: mpsc::Sender<Result<Bytes, std::io::Error>>,
    next_index: u64,
) {
    let WeaselCorrectionCtx {
        state,
        upstream_url,
        forward_headers,
        original_body,
        original_request_id,
        session_id,
        correction_prompt,
    } = ctx;

    log_weasel_correction_fired(original_request_id, &trigger_word);

    let correction = CorrectionRequest::new(
        original_body,
        weasel_text,
        correction_prompt,
        trigger_word.clone(),
    );
    let correction_body = match correction.build_body() {
        Ok(b) => b,
        Err(e) => {
            warn_correction_body_build_failed(original_request_id, &e);
            let _ = body_tx
                .send(Ok(stitch::error_frame(
                    "the mike proxy cut this reply at a listed phrase and could not build the request for a corrected one",
                )))
                .await;
            return;
        }
    };

    let correction_created_at = chrono::Utc::now();

    let lane = LaneIdentity::of_request(&state, original_request_id).await;
    let mut minimal = make_correction_intercepted_request(
        RequestId::next(),
        session_id,
        correction_body.clone(),
        weasel_correction_prompt(&trigger_word),
        correction_created_at,
        lane,
    );

    let correction_request_id =
        RequestStore::insert_request_stub_to_db(&state.store, &minimal, false).await;
    minimal.id = correction_request_id;

    {
        let mut store = state.store.write().await;
        store.insert(minimal);
    }

    {
        let mut store = state.store.write().await;
        store.update(&original_request_id, |req| {
            req.intercepted = true;
            req.correction_request_id = Some(correction_request_id.as_u64());
        });
    }
    if let Some(persist) = state.persist() {
        persist
            .mark_intercepted(
                original_request_id.as_u64() as i64,
                correction_request_id.as_u64() as i64,
            )
            .await;
    }

    let upstream_resp = match state
        .client
        .post(upstream_url)
        .headers(forward_headers)
        .body(correction_body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            warn_correction_upstream_failed(original_request_id, &e);
            let _ = body_tx
                .send(Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionRefused,
                    e.to_string(),
                )))
                .await;
            return;
        }
    };

    spawn_stitched_tee(
        &state,
        upstream_resp,
        body_tx,
        correction_request_id,
        next_index,
    );
}

fn log_weasel_correction_fired(request_id: RequestId, trigger_word: &str) {
    tracing::info!(
        request_id = %request_id,
        trigger_word = %trigger_word,
        "weasel intercept fired — cutting the reply and asking for a corrected one"
    );
}

fn warn_correction_body_build_failed(request_id: RequestId, e: &impl std::fmt::Display) {
    tracing::warn!(
        request_id = %request_id,
        error = %e,
        "weasel correction body build failed — ending the reply with an error"
    );
}

fn warn_correction_upstream_failed(request_id: RequestId, e: &impl std::fmt::Display) {
    tracing::warn!(
        request_id = %request_id,
        error = %e,
        "weasel correction upstream request failed"
    );
}
