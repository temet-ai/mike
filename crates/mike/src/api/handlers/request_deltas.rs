use super::*;

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct DeltaRowDto {
    pub(crate) change_kind: String,
    pub(crate) ord: i32,
    pub(crate) message_id: i64,
    pub(crate) message_sha256: String,
    pub(crate) role: String,
    pub(crate) byte_size: i32,
    pub(crate) body: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) removed_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct RequestDeltasPayload {
    pub(crate) request_id: u64,
    pub(crate) predecessor_request_id: Option<i64>,
    pub(crate) deltas: Vec<DeltaRowDto>,
}

impl From<crate::store::persistence::messages::PersistedDeltaRow> for DeltaRowDto {
    fn from(row: crate::store::persistence::messages::PersistedDeltaRow) -> Self {
        Self {
            change_kind: row.change_kind,
            ord: row.ord,
            message_id: row.message_id,
            message_sha256: row.message_sha256,
            role: row.role,
            byte_size: row.byte_size,
            body: row.body,
            removed_reason: row.removed_reason,
        }
    }
}

pub(crate) async fn get_request_deltas(
    State(state): State<AppState>,
    Path((session_id, request_id)): Path<(String, u64)>,
) -> Result<Json<RequestDeltasPayload>, Response> {
    if uuid::Uuid::parse_str(&session_id).is_err() {
        return Err((StatusCode::NOT_FOUND, "unknown session").into_response());
    }
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };

    let session_db_id = match handle.resolve_session_db_id(&session_id).await {
        Ok(Some(db_id)) => db_id,
        Ok(None) => return Err((StatusCode::NOT_FOUND, "unknown session").into_response()),
        Err(e) => {
            tracing::warn!(
                error = %e,
                session_id = %session_id,
                "get_request_deltas: session resolve failed",
            );
            return Err((StatusCode::INTERNAL_SERVER_ERROR, "deltas query failed").into_response());
        }
    };

    let id_i64 = i64::try_from(request_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "request id out of range").into_response())?;
    let rows = handle
        .fetch_request_deltas_by_id(id_i64, session_db_id)
        .await
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                request_id,
                session_id = %session_id,
                "get_request_deltas: DB fetch failed",
            );
            (StatusCode::INTERNAL_SERVER_ERROR, "deltas query failed").into_response()
        })?;
    if rows.is_empty() {
        let existing = handle.fetch_request_by_id(request_id).await.map_err(|e| {
            tracing::warn!(
                error = %e,
                request_id,
                session_id = %session_id,
                "get_request_deltas: existence probe failed",
            );
            (StatusCode::INTERNAL_SERVER_ERROR, "deltas query failed").into_response()
        })?;
        match existing {
            Some(req) if req.session_id == session_id => {}
            _ => return Err((StatusCode::NOT_FOUND, "unknown request").into_response()),
        }
        return Ok(Json(RequestDeltasPayload {
            request_id,
            predecessor_request_id: None,
            deltas: Vec::new(),
        }));
    }
    let predecessor_request_id = rows[0].predecessor_request_id;
    let deltas: Vec<DeltaRowDto> = rows.into_iter().map(DeltaRowDto::from).collect();
    Ok(Json(RequestDeltasPayload {
        request_id,
        predecessor_request_id,
        deltas,
    }))
}

pub(crate) async fn get_stream_chunks(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<crate::store::models::RequestStreamBlocksPayload>, StatusCode> {
    use crate::store::models::{RequestStreamBlocksPayload, StreamBlockDto};
    let Some(handle) = state.persist() else {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };
    let id_i64 = i64::try_from(id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let rows = handle
        .fetch_stream_blocks_for_request(id_i64)
        .await
        .map_err(|e| {
            tracing::warn!(
                error = %e,
                request_id = id,
                "get_stream_chunks: DB fetch failed",
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if rows.is_empty() {
        let row_exists = handle.fetch_request_by_id(id).await.map_err(|e| {
            tracing::warn!(
                error = %e,
                request_id = id,
                "get_stream_chunks: existence probe failed",
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        if row_exists.is_none() {
            return Err(StatusCode::NOT_FOUND);
        }
        return Ok(Json(RequestStreamBlocksPayload {
            request_id: id,
            stream_blocks: Vec::new(),
        }));
    }
    let stream_blocks: Vec<StreamBlockDto> = rows
        .into_iter()
        .map(|r| StreamBlockDto {
            seq_ord: r.seq_ord,
            content: r.content,
            byte_size: r.byte_size,
            content_sha256: r.content_sha256,
            block_type: r.block_type,
            signature: r.signature,
            wire_tool_use_id: r.wire_tool_use_id,
            arrived_at: r.arrived_at,
        })
        .collect();
    Ok(Json(RequestStreamBlocksPayload {
        request_id: id,
        stream_blocks,
    }))
}
