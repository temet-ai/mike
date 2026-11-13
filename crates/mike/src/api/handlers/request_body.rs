use super::*;
use crate::store::persistence::steerer_repo::SteererPersist as _;

pub(crate) fn byte_size(bytes: Option<&Bytes>) -> u64 {
    bytes.map(|b| b.len() as u64).unwrap_or(0)
}

pub(crate) fn decode_utf8(bytes: Option<&Bytes>) -> Option<String> {
    bytes.and_then(|b| std::str::from_utf8(b).ok().map(|s| s.to_owned()))
}

#[derive(Serialize)]
pub(crate) struct BodyPayload {
    pub(crate) text: Option<String>,
    pub(crate) byte_size: u64,
    pub(crate) has_body: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pointer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pointer_resolved: Option<bool>,
    pub(crate) reconstructed: bool,
}

impl BodyPayload {
    pub(crate) fn from_bytes(body: Option<&Bytes>) -> Self {
        let has_body = body.is_some();
        let byte_size = byte_size(body);
        Self {
            text: decode_utf8(body),
            byte_size,
            has_body,
            pointer: None,
            pointer_resolved: None,
            reconstructed: false,
        }
    }

    pub(crate) fn from_bytes_with_pointer(body: Option<&Bytes>, pointer: &str) -> Self {
        let pointer_owned = pointer.to_string();
        let has_body = body.is_some();
        let raw_size = byte_size(body);
        let Some(bytes) = body else {
            return Self {
                text: None,
                byte_size: 0,
                has_body,
                pointer: Some(pointer_owned),
                pointer_resolved: Some(false),
                reconstructed: false,
            };
        };
        let parsed: Result<serde_json::Value, _> = serde_json::from_slice(bytes);
        let parsed = match parsed {
            Ok(v) => v,
            Err(_) => {
                return Self {
                    text: None,
                    byte_size: raw_size,
                    has_body,
                    pointer: Some(pointer_owned),
                    pointer_resolved: Some(false),
                    reconstructed: false,
                };
            }
        };
        let Some(node) = parsed.pointer(pointer) else {
            return Self {
                text: None,
                byte_size: 0,
                has_body,
                pointer: Some(pointer_owned),
                pointer_resolved: Some(false),
                reconstructed: false,
            };
        };
        let text = serde_json::to_string_pretty(node).unwrap_or_else(|_| node.to_string());
        let projected_size = text.len() as u64;
        Self {
            text: Some(text),
            byte_size: projected_size,
            has_body,
            pointer: Some(pointer_owned),
            pointer_resolved: Some(true),
            reconstructed: false,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct BodyRouteDescriptor {
    pub(crate) href: String,
    pub(crate) description: &'static str,
    pub(crate) supports_pointer: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) query_hint: Option<&'static str>,
}

#[derive(Serialize)]
pub(crate) struct RequestBodyIndexPayload {
    pub(crate) request_id: u64,
    pub(crate) request: BodyRouteDescriptor,
    pub(crate) request_sent: BodyRouteDescriptor,
    pub(crate) response: BodyRouteDescriptor,
    pub(crate) transformation_manifest: BodyRouteDescriptor,
    pub(crate) find: BodyRouteDescriptor,
}

#[derive(Deserialize, Default)]
pub(crate) struct BodyPointerParams {
    #[serde(default)]
    pub(crate) pointer: Option<String>,
    #[serde(default)]
    pub(crate) tool_use_id: Option<String>,
    #[serde(default)]
    pub(crate) sha256: Option<String>,
}

impl BodyPointerParams {
    pub(crate) fn effective_pointer(&self) -> Option<&str> {
        self.pointer
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }
}

pub(crate) async fn get_request_body_index(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<RequestBodyIndexPayload>, StatusCode> {
    let in_memory_exists = {
        let store = state.store.read().await;
        store.get(&RequestId::from_raw(id)).is_some()
    };
    if !in_memory_exists && load_request_from_db(&state, id).await?.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    let base = format!("/api/requests/{id}/body");
    Ok(Json(RequestBodyIndexPayload {
        request_id: id,
        request: BodyRouteDescriptor {
            href: format!("{base}/request"),
            description: "Raw request body the client sent.",
            supports_pointer: true,
            query_hint: Some("?pointer=/messages/0/content/0/text"),
        },
        request_sent: BodyRouteDescriptor {
            href: format!("{base}/request_sent"),
            description: "Body forwarded upstream after request filtering.",
            supports_pointer: true,
            query_hint: Some("?pointer=/messages/0/content/0/text"),
        },
        response: BodyRouteDescriptor {
            href: format!("{base}/response"),
            description: "Captured response body, or 404 Pending while the request is still in flight.",
            supports_pointer: true,
            query_hint: Some("?pointer=/content/0/text"),
        },
        transformation_manifest: BodyRouteDescriptor {
            href: format!("/api/requests/{id}/manifest"),
            description: "Full transformation manifest: original->sent hash chain and per-stage outcomes. has_manifest:false + manifest:null when none was recorded.",
            supports_pointer: true,
            query_hint: Some("?pointer=/stages/0"),
        },
        find: BodyRouteDescriptor {
            href: format!("{base}/find"),
            description: "Substring search over request, filtered-request, or response bodies.",
            supports_pointer: false,
            query_hint: Some("?needle=foo&source=request"),
        },
    }))
}

pub(crate) async fn get_request_body(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Query(params): Query<BodyPointerParams>,
) -> Result<Json<BodyPayload>, StatusCode> {
    let in_memory = {
        let store = state.store.read().await;
        store.get(&RequestId::from_raw(id)).cloned()
    };
    let (body, reconstructed) = match in_memory {
        Some(req) => (req.request_body.clone(), false),
        None => match load_request_body_from_db(&state, id).await? {
            Some(hit) => hit,
            None => return Err(StatusCode::NOT_FOUND),
        },
    };
    let recovery_pointer = original_result_pointer(&params, body.as_ref())?;
    let mut payload = match recovery_pointer.as_deref().or(params.effective_pointer()) {
        Some(p) => BodyPayload::from_bytes_with_pointer(body.as_ref(), p),
        None => BodyPayload::from_bytes(body.as_ref()),
    };
    payload.reconstructed = reconstructed;
    Ok(Json(payload))
}

pub(crate) async fn get_tool_result_original(
    State(state): State<AppState>,
    Path(tool_use_id): Path<String>,
    Query(params): Query<BodyPointerParams>,
) -> Result<Json<BodyPayload>, StatusCode> {
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let request_id = handle
        .steerer()
        .first_request_carrying_result(&tool_use_id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, tool_use_id, "get_tool_result_original: lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    let params = BodyPointerParams {
        pointer: None,
        tool_use_id: Some(tool_use_id),
        sha256: params.sha256,
    };
    get_request_body(State(state), Path(request_id as u64), Query(params)).await
}

fn original_result_pointer(
    params: &BodyPointerParams,
    body: Option<&Bytes>,
) -> Result<Option<String>, StatusCode> {
    match (&params.tool_use_id, &params.sha256) {
        (None, None) => Ok(None),
        (Some(id), Some(hash))
            if params.pointer.is_none()
                && !id.is_empty()
                && hash.len() == 64
                && hash.bytes().all(|c| c.is_ascii_hexdigit()) =>
        {
            let body = body.ok_or(StatusCode::NOT_FOUND)?;
            crate::compress::recovery::CapturedResults::new(body)
                .pointer(id, &hash.to_ascii_lowercase())
                .map(|p| Some(p.to_string()))
                .ok_or(StatusCode::CONFLICT)
        }
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

pub(crate) async fn load_request_body_from_db(
    state: &AppState,
    id: u64,
) -> Result<Option<(Option<Bytes>, bool)>, StatusCode> {
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    if !request_row_exists(handle, id).await? {
        return Ok(None);
    }
    fetch_request_body_bytes(handle, id).await.map(Some)
}

async fn request_row_exists(
    handle: &crate::store::persistence::PersistHandle,
    id: u64,
) -> Result<bool, StatusCode> {
    match handle.fetch_request_by_id(id).await {
        Ok(opt) => Ok(opt.is_some()),
        Err(e) => {
            tracing::warn!(
                error = %e,
                request_id = id,
                "get_request_body: DB fallback metadata lookup failed"
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn fetch_request_body_bytes(
    handle: &crate::store::persistence::PersistHandle,
    id: u64,
) -> Result<(Option<Bytes>, bool), StatusCode> {
    match handle.fetch_request_body_by_id(id).await {
        Ok(Some(s)) => Ok((Some(s.bytes), s.reconstructed)),
        Ok(None) => Ok((None, false)),
        Err(e) => {
            tracing::warn!(
                error = %e,
                request_id = id,
                "get_request_body: DB fallback blob fetch failed"
            );
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct AccumulatedTextPayload {
    pub(crate) text: String,
}

pub(crate) async fn get_accumulated_text(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<AccumulatedTextPayload>, StatusCode> {
    let Some(handle) = state.persist() else {
        return Err(StatusCode::NOT_FOUND);
    };
    let id_i64 = i64::try_from(id).unwrap_or(i64::MAX);
    let text = handle
        .fetch_accumulated_text_for_request(id_i64)
        .await
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                request_id = id,
                "get_accumulated_text: DB fetch failed",
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let Some(text) = text else {
        return Err(StatusCode::NOT_FOUND);
    };
    Ok(Json(AccumulatedTextPayload { text }))
}

pub(crate) async fn get_request_sent_body(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Query(params): Query<BodyPointerParams>,
) -> Result<Json<BodyPayload>, StatusCode> {
    let body = resolve_request_sent_body(&state, id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    let payload = match params.effective_pointer() {
        Some(p) => BodyPayload::from_bytes_with_pointer(body.as_ref(), p),
        None => BodyPayload::from_bytes(body.as_ref()),
    };
    Ok(Json(payload))
}

pub(crate) async fn resolve_request_sent_body(
    state: &AppState,
    id: u64,
) -> Result<Option<Option<Bytes>>, StatusCode> {
    let in_memory = {
        let store = state.store.read().await;
        store.get(&RequestId::from_raw(id)).cloned()
    };
    match in_memory {
        Some(req) => Ok(Some(
            req.sent_body
                .or(req.filtered_request_body)
                .or(req.request_body),
        )),
        None => {
            let Some(req) = load_request_from_db(state, id).await? else {
                return Ok(None);
            };
            let body = match state.persist() {
                Some(handle) => handle.fetch_request_sent_body_by_id(id).await.map_err(|e| {
                    tracing::warn!(error = %e, request_id = id, "request_sent reconstruction failed");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?,
                None => req.request_body,
            };
            Ok(Some(body))
        }
    }
}
