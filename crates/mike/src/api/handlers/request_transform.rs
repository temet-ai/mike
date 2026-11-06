use super::*;

#[derive(Serialize)]
pub(crate) struct TransformationManifestPayload {
    pub(crate) request_id: u64,
    pub(crate) has_manifest: bool,
    pub(crate) manifest: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pointer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pointer_resolved: Option<bool>,
}

pub(crate) async fn get_request_transformation_manifest(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Query(params): Query<BodyPointerParams>,
) -> Result<Json<TransformationManifestPayload>, StatusCode> {
    let in_memory = {
        let store = state.store.read().await;
        store.get(&RequestId::from_raw(id)).cloned()
    };
    let req = match in_memory {
        Some(req) => req,
        None => match load_request_from_db(&state, id).await? {
            Some(req) => req,
            None => return Err(StatusCode::NOT_FOUND),
        },
    };
    let Some(manifest) = req.transformation_manifest else {
        return Ok(Json(TransformationManifestPayload {
            request_id: id,
            has_manifest: false,
            manifest: None,
            pointer: None,
            pointer_resolved: None,
        }));
    };
    let payload = match params.effective_pointer() {
        Some(p) => {
            let (resolved, resolved_ok) = match manifest.pointer(p) {
                Some(node) => (Some(node.clone()), true),
                None => (None, false),
            };
            TransformationManifestPayload {
                request_id: id,
                has_manifest: true,
                manifest: resolved,
                pointer: Some(p.to_string()),
                pointer_resolved: Some(resolved_ok),
            }
        }
        None => TransformationManifestPayload {
            request_id: id,
            has_manifest: true,
            manifest: Some(manifest),
            pointer: None,
            pointer_resolved: None,
        },
    };
    Ok(Json(payload))
}

pub(crate) async fn get_response_body(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Query(params): Query<BodyPointerParams>,
) -> Response {
    let body = match load_response_body_from_db(&state, id).await {
        Ok(Some(hit)) => hit,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(r) => return r,
    };
    let payload = match params.effective_pointer() {
        Some(p) => BodyPayload::from_bytes_with_pointer(body.as_ref(), p),
        None => BodyPayload::from_bytes(body.as_ref()),
    };
    Json(payload).into_response()
}

async fn fetch_response_meta_row(
    handle: &crate::store::persistence::PersistHandle,
    id: u64,
) -> Result<Option<InterceptedRequest>, Response> {
    handle.fetch_request_by_id(id).await.map_err(|e| {
        tracing::warn!(
            error = %e,
            request_id = id,
            "get_response_body: DB fallback metadata lookup failed"
        );
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    })
}

fn pending_response_error() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "error": "response not yet available",
            "status": "Pending"
        })),
    )
        .into_response()
}

pub(crate) async fn load_response_body_from_db(
    state: &AppState,
    id: u64,
) -> Result<Option<Option<Bytes>>, Response> {
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE.into_response());
    };
    let Some(meta) = fetch_response_meta_row(handle, id).await? else {
        return Ok(None);
    };
    if matches!(meta.status, RequestStatus::Pending) {
        return Err(pending_response_error());
    }
    match handle.fetch_response_body_by_id(id).await {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) => {
            tracing::warn!(
                error = %e,
                request_id = id,
                "get_response_body: DB fallback blob fetch failed"
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
    }
}
