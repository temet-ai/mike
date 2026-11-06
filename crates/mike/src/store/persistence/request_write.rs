use serde_json::Value as JsonValue;

use super::PersistError;
use super::capture_classification::classify_author_kind;
use super::id_resolvers::resolve_session_and_project_id;
use super::{messages, reconstruct};
use crate::store::models::{self, InterceptedRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowInsert {
    Inserted,
    AlreadyExisted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdMode {
    Mint,
    Ensure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestInsertOutcome {
    pub id: i64,
    pub row: RowInsert,
    pub session_id: Option<i64>,
}

pub(crate) struct RequestEnvelopePrep<'a> {
    pub(crate) request_body_bytes: Option<&'a [u8]>,
    pub(crate) request_body_size: i32,
    pub(crate) request_envelope: Option<Vec<u8>>,
    pub(crate) filtered_request_body_bytes: Option<&'a [u8]>,
    pub(crate) envelope_patch: Option<messages::SentPatch>,
    pub(crate) request_headers: JsonValue,
    pub(crate) params: JsonValue,
    pub(crate) message_count: i32,
    pub(crate) request_id: i64,
    pub(crate) author_kind_code: &'static str,
}

impl<'a> RequestEnvelopePrep<'a> {
    pub(crate) fn compute(req: &'a InterceptedRequest) -> Result<Self, PersistError> {
        let request_body_bytes = req.request_body.as_deref();
        let request_body_size = request_body_bytes.map(|b| b.len() as i32).unwrap_or(0);
        let request_envelope = request_body_bytes.and_then(reconstruct::strip_bulk_arrays);
        let filtered_request_body_bytes =
            match (req.filter_applied, req.filtered_request_body.as_ref()) {
                (true, Some(bytes)) => Some(bytes.as_ref()),
                _ => None,
            };
        let envelope_patch = compute_envelope_patch_for_insert(req, request_body_bytes);

        let request_headers = serde_json::to_value(&req.request_headers)?;
        let params = serde_json::to_value(&req.params)?;
        let message_count = req.message_count as i32;
        let request_id = req.id.as_u64() as i64;
        let author_kind_code = classify_author_kind(
            &req.turn_origin,
            &req.human_prompt,
            &req.last_user_message,
            req.is_tool_loop,
            req.claude_code_agent_id.is_some(),
        );

        Ok(Self {
            request_body_bytes,
            request_body_size,
            request_envelope,
            filtered_request_body_bytes,
            envelope_patch,
            request_headers,
            params,
            message_count,
            request_id,
            author_kind_code,
        })
    }
}

fn compute_envelope_patch_for_insert(
    req: &InterceptedRequest,
    request_body_bytes: Option<&[u8]>,
) -> Option<messages::SentPatch> {
    let (orig, sent) = match (request_body_bytes, req.sent_body.as_deref()) {
        (Some(orig), Some(sent)) => (orig, sent),
        _ => return None,
    };
    match messages::compute_envelope_patch(orig, sent) {
        Ok(p) => {
            warn_if_envelope_patch_missing(req, orig, sent, p.as_ref());
            p
        }
        Err(e) => {
            tracing::warn!(
                request_id = req.id.as_u64(),
                error = %e,
                "insert_request_initial: envelope sent-patch computation skipped"
            );
            None
        }
    }
}

fn warn_if_envelope_patch_missing(
    req: &InterceptedRequest,
    orig: &[u8],
    sent: &[u8],
    patch: Option<&messages::SentPatch>,
) {
    if patch.is_none() && orig != sent {
        tracing::warn!(
            request_id = req.id.as_u64(),
            "insert_request_initial: sent body differs from request body but \
             compute_envelope_patch found no single-field patch to capture; \
             request_sent reconstruction will fall back to the unmutated request_body"
        );
    }
}

pub(crate) fn fallback_id_skip_outcome(
    id_mode: IdMode,
    req: &InterceptedRequest,
) -> Option<RequestInsertOutcome> {
    if matches!(id_mode, IdMode::Ensure) && models::is_fallback(req.id.as_u64()) {
        tracing::warn!(
            request_id = req.id.as_u64(),
            "skipping initial persist for fallback request id (stub INSERT was in DB-down mode)"
        );
        return Some(RequestInsertOutcome {
            id: req.id.as_u64() as i64,
            row: RowInsert::AlreadyExisted,
            session_id: None,
        });
    }
    None
}

pub(crate) async fn resolve_capture_session_and_project(
    tx: &mut sqlx::PgConnection,
    req: &InterceptedRequest,
    is_new_session: bool,
) -> Result<(i64, i64), PersistError> {
    let message_count = Some(req.message_count as i32);
    let existing_uuid = if is_new_session {
        None
    } else {
        req.session_id.parse::<uuid::Uuid>().ok()
    };
    if let Some(session_uuid) = existing_uuid {
        let existing: Option<(i64, i64)> = sqlx::query!(
            "SELECT id, project_id FROM sessions.sessions WHERE session_uuid = $1",
            session_uuid
        )
        .fetch_optional(&mut *tx)
        .await?
        .map(|row| (row.id, row.project_id));
        if let Some(found) = existing {
            return Ok(found);
        }
    }
    resolve_session_and_project_id(&mut *tx, &req.session_id, None, message_count).await
}

async fn insert_requests_row_mint(
    tx: &mut sqlx::PgConnection,
    req: &InterceptedRequest,
    session_db_id: i64,
    project_id: i64,
    prep: &RequestEnvelopePrep<'_>,
) -> Result<Option<i64>, PersistError> {
    let envelope_patch_start = prep.envelope_patch.as_ref().map(|p| p.start);
    let envelope_patch_end = prep.envelope_patch.as_ref().map(|p| p.end);
    let envelope_patch_bytes = prep.envelope_patch.as_ref().map(|p| p.bytes.as_slice());
    let returned_id: Option<i64> = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            session_id, project_id,
            method_id, path, model, is_streaming, message_count, is_tool_loop,
            status_id,
            request_headers,
            params,
            request_body,
            request_body_size,
            last_user_message, human_prompt,
            turn_type_id, turn_origin_id, subagent_persona,
            created_at,
            filtered_request_body,
            agent_instance_id,
            claude_code_agent_id,
            transformation_manifest,
            sent_patch_start, sent_patch_end, sent_patch_bytes,
            author_kind_id,
            request_envelope
        )
        SELECT $1, $2, rm.id, $4, $5, $6, $7, $8, rs.id,
               $9, $10, $11, $25, $12, $13, rtt.id, rto.id, $16, $17, $18, $19, $20, $21,
               $22, $23, $24, ak.id, $27
        FROM requests.methods rm, requests.statuses rs,
             requests.turn_types rtt, requests.turn_origins rto,
             requests.author_kinds ak
        WHERE rm.code = $3 AND rs.code = 'pending'
          AND rtt.code = $14 AND rto.code = $15 AND ak.code = $26
        RETURNING id",
    )
    .bind(session_db_id)
    .bind(project_id)
    .bind(&req.method)
    .bind(&req.path)
    .bind(&req.model)
    .bind(req.is_streaming)
    .bind(prep.message_count)
    .bind(req.is_tool_loop)
    .bind(&prep.request_headers)
    .bind(&prep.params)
    .bind(prep.request_body_bytes)
    .bind(&req.last_user_message)
    .bind(&req.human_prompt)
    .bind(&req.turn_type)
    .bind(&req.turn_origin)
    .bind(&req.subagent_persona)
    .bind(req.created_at)
    .bind(prep.filtered_request_body_bytes)
    .bind(req.agent_instance_id)
    .bind(&req.claude_code_agent_id)
    .bind(req.transformation_manifest.as_ref())
    .bind(envelope_patch_start)
    .bind(envelope_patch_end)
    .bind(envelope_patch_bytes)
    .bind(prep.request_body_size)
    .bind(prep.author_kind_code)
    .bind(prep.request_envelope.as_deref())
    .fetch_optional(tx)
    .await?;
    Ok(returned_id)
}

async fn insert_requests_row_ensure(
    tx: &mut sqlx::PgConnection,
    req: &InterceptedRequest,
    session_db_id: i64,
    project_id: i64,
    prep: &RequestEnvelopePrep<'_>,
) -> Result<Option<i64>, PersistError> {
    let envelope_patch_start = prep.envelope_patch.as_ref().map(|p| p.start);
    let envelope_patch_end = prep.envelope_patch.as_ref().map(|p| p.end);
    let envelope_patch_bytes = prep.envelope_patch.as_ref().map(|p| p.bytes.as_slice());
    let returned_id: Option<i64> = sqlx::query_scalar(
        "INSERT INTO requests.requests (
            id,
            session_id, project_id,
            method_id, path, model, is_streaming, message_count, is_tool_loop,
            status_id,
            request_headers,
            params,
            request_body,
            request_body_size,
            last_user_message, human_prompt,
            turn_type_id, turn_origin_id, subagent_persona,
            created_at,
            filtered_request_body,
            agent_instance_id,
            claude_code_agent_id,
            transformation_manifest,
            sent_patch_start, sent_patch_end, sent_patch_bytes,
            author_kind_id,
            request_envelope
        )
        OVERRIDING SYSTEM VALUE
        SELECT $1, $2, $3, rm.id, $5, $6, $7, $8, $9, rs.id,
               $10, $11, $12, $26, $13, $14, rtt.id, rto.id, $17, $18, $19, $20, $21, $22,
               $23, $24, $25, ak.id, $28
        FROM requests.methods rm, requests.statuses rs,
             requests.turn_types rtt, requests.turn_origins rto,
             requests.author_kinds ak
        WHERE rm.code = $4 AND rs.code = 'pending'
          AND rtt.code = $15 AND rto.code = $16 AND ak.code = $27
        ON CONFLICT ON CONSTRAINT requests_pkey DO NOTHING
        RETURNING id",
    )
    .bind(prep.request_id)
    .bind(session_db_id)
    .bind(project_id)
    .bind(&req.method)
    .bind(&req.path)
    .bind(&req.model)
    .bind(req.is_streaming)
    .bind(prep.message_count)
    .bind(req.is_tool_loop)
    .bind(&prep.request_headers)
    .bind(&prep.params)
    .bind(prep.request_body_bytes)
    .bind(&req.last_user_message)
    .bind(&req.human_prompt)
    .bind(&req.turn_type)
    .bind(&req.turn_origin)
    .bind(&req.subagent_persona)
    .bind(req.created_at)
    .bind(prep.filtered_request_body_bytes)
    .bind(req.agent_instance_id)
    .bind(&req.claude_code_agent_id)
    .bind(req.transformation_manifest.as_ref())
    .bind(envelope_patch_start)
    .bind(envelope_patch_end)
    .bind(envelope_patch_bytes)
    .bind(prep.request_body_size)
    .bind(prep.author_kind_code)
    .bind(prep.request_envelope.as_deref())
    .fetch_optional(tx)
    .await?;
    Ok(returned_id)
}

pub(crate) async fn insert_requests_row(
    tx: &mut sqlx::PgConnection,
    req: &InterceptedRequest,
    id_mode: IdMode,
    session_db_id: i64,
    project_id: i64,
    prep: &RequestEnvelopePrep<'_>,
) -> Result<Option<i64>, PersistError> {
    match id_mode {
        IdMode::Mint => insert_requests_row_mint(tx, req, session_db_id, project_id, prep).await,
        IdMode::Ensure => {
            insert_requests_row_ensure(tx, req, session_db_id, project_id, prep).await
        }
    }
}
