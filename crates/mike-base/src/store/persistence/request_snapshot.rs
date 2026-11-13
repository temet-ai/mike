use serde_json::Value as JsonValue;

use super::assemble;
use super::id_resolvers::resolve_session_and_project_id;
use super::request_snapshot_prep::{SnapshotPrep, extract_title_for_snapshot};
use super::turn_lifecycle::{self, TurnLifecycleCtx};
use super::{PersistError, PersistHandle};
use crate::store::models::{self, InterceptedRequest};

async fn update_request_status_and_error(
    tx: &mut sqlx::PgConnection,
    prep: &SnapshotPrep,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<(), PersistError> {
    sqlx::query!(
        "UPDATE requests.requests r
         SET status_id    = st.id,
             completed_at = $3
        FROM requests.statuses st
       WHERE st.code = $2
         AND r.id    = $1",
        prep.request_id,
        prep.status_code,
        completed_at,
    )
    .execute(&mut *tx)
    .await?;
    if let Some(error_message) = &prep.status_error_message {
        sqlx::query!(
            "INSERT INTO requests.errors (request_id, message)
             SELECT $1, $2
             WHERE NOT EXISTS (
                 SELECT 1 FROM requests.errors WHERE request_id = $1
             )",
            prep.request_id,
            error_message,
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

async fn check_stream_blocks_gapped(
    tx: &mut sqlx::PgConnection,
    request_id: i64,
    expected_stream_block_count: u32,
) -> Result<bool, PersistError> {
    if expected_stream_block_count == 0 {
        return Ok(false);
    }
    let committed_blocks: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM responses.stream_blocks WHERE request_id = $1",
        request_id,
    )
    .fetch_one(&mut *tx)
    .await?
    .unwrap_or(0);
    let gapped = committed_blocks < i64::from(expected_stream_block_count);
    if gapped {
        tracing::error!(
            request_id,
            expected = expected_stream_block_count,
            "stream blocks incomplete after terminal ENSURE retry — response reconstruction has a hole"
        );
    }
    Ok(gapped)
}

struct TerminalResponseInsert<'a> {
    request_id: i64,
    response_status: i16,
    response_headers: &'a JsonValue,
    response_body: &'a [u8],
    upstream_request_id: Option<&'a str>,
    upstream_message_id: Option<&'a str>,
    token_usage: &'a JsonValue,
    stop_reason: Option<&'a str>,
    stop_details: Option<&'a JsonValue>,
    message_extras: Option<&'a JsonValue>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
    stream_blocks_gapped: bool,
}

async fn insert_terminal_response_row(
    tx: &mut sqlx::PgConnection,
    row: &TerminalResponseInsert<'_>,
) -> Result<(), PersistError> {
    sqlx::query!(
        "INSERT INTO responses.responses (
            request_id,
            response_status, response_headers, response_body,
            upstream_request_id, upstream_message_id,
            token_usage, stop_reason_id, stop_details, completed_at,
            stream_blocks_gapped, message_extras
         )
         SELECT $1, $2, $3, $4, $5, $6, $7, sr.id, $9, $10, $11, $12
         FROM (SELECT $8::text AS code) AS v
         LEFT JOIN responses.stop_reasons sr ON sr.code = v.code
         WHERE NOT EXISTS (
             SELECT 1 FROM responses.responses WHERE request_id = $1
         )",
        row.request_id,
        row.response_status,
        row.response_headers,
        row.response_body,
        row.upstream_request_id,
        row.upstream_message_id,
        row.token_usage,
        row.stop_reason,
        row.stop_details,
        row.completed_at,
        row.stream_blocks_gapped,
        row.message_extras,
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}

async fn lookup_request_db_id(
    tx: &mut sqlx::PgConnection,
    request_id: i64,
    created_at: chrono::DateTime<chrono::Utc>,
) -> Result<Option<i64>, PersistError> {
    let request_db_id: Option<i64> = sqlx::query_scalar!(
        "SELECT id FROM requests.requests WHERE id = $1 AND created_at = $2",
        request_id,
        created_at,
    )
    .fetch_optional(&mut *tx)
    .await?;
    Ok(request_db_id)
}

async fn insert_request_diagnostics(
    tx: &mut sqlx::PgConnection,
    request_db_id: i64,
    diagnostics: &JsonValue,
) -> Result<(), PersistError> {
    sqlx::query!(
        "INSERT INTO requests.diagnostics (
            request_id, category, analyzer, advice,
            matched_text, match_range, other_hits, created_at
         )
         SELECT $1,
                d->>'category', d->>'analyzer', d->>'advice',
                d->>'matched_text', d->'match_range',
                COALESCE(d->'other_hits', '[]'::jsonb),
                COALESCE((d->>'created_at')::timestamptz, NOW())
         FROM jsonb_array_elements($2::jsonb) d
         WHERE NOT EXISTS (
             SELECT 1 FROM requests.diagnostics WHERE request_id = $1
         )",
        request_db_id,
        diagnostics,
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}

impl PersistHandle {
    pub async fn resolve_title_for_snapshot(
        &self,
        req: &InterceptedRequest,
    ) -> Result<Option<String>, PersistError> {
        if let Some(title) = extract_title_for_snapshot(req) {
            return Ok(Some(title));
        }
        if req.turn_origin != "title_sidecar"
            || !req.is_streaming
            || !matches!(req.status, crate::store::models::RequestStatus::Complete)
        {
            return Ok(None);
        }
        let Some(text) = self
            .fetch_accumulated_text_for_request(req.id.as_u64() as i64)
            .await?
        else {
            return Ok(None);
        };
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) else {
            return Ok(None);
        };
        let Some(title) = parsed.get("title").and_then(|v| v.as_str()) else {
            return Ok(None);
        };
        let title = title.trim();
        if title.is_empty() {
            Ok(None)
        } else {
            Ok(Some(title.to_string()))
        }
    }

    async fn assemble_streaming_body_if_needed(
        &self,
        req: &InterceptedRequest,
        request_id: i64,
        prep: &SnapshotPrep,
    ) -> Result<Option<Vec<u8>>, PersistError> {
        let body_is_empty = req.response_body.as_deref().is_none_or(<[u8]>::is_empty);
        if !(req.is_streaming && body_is_empty) {
            return Ok(None);
        }
        let envelope = assemble::MessageEnvelope {
            upstream_message_id: req.upstream_message_id.clone(),
            model: req.model.clone(),
            stop_reason: req.stop_reason.clone(),
            token_usage: prep.token_usage.clone(),
            message_extras: prep.message_extras.clone(),
        };
        match assemble::assemble_message_with_envelope(&self.pool, request_id, envelope).await? {
            Some(value) => Ok(Some(serde_json::to_vec(&value)?)),
            None => Ok(None),
        }
    }

    async fn roll_session_summary_forward(
        &self,
        session_db_id: i64,
        req: &InterceptedRequest,
        status_str: &str,
        title: Option<&str>,
    ) {
        if let Err(e) = sqlx::query!(
            "UPDATE sessions.sessions
             SET last_seen   = GREATEST(last_seen, $2),
                 last_status = $3,
                 title       = COALESCE($4, title)
             WHERE id = $1",
            session_db_id,
            req.created_at,
            status_str,
            title,
        )
        .execute(&self.pool)
        .await
        {
            tracing::warn!(
                session_id = %req.session_id,
                error = %e,
                "post-commit sessions UPDATE failed — summary stale until next request"
            );
        }
    }

    pub async fn snapshot_request(
        &self,
        req: &InterceptedRequest,
    ) -> Result<(Option<String>, Option<i64>), PersistError> {
        crate::metrics::time_db("snapshot_request", async move {
            if models::is_fallback(req.id.as_u64()) {
                tracing::warn!(
                    request_id = req.id.as_u64(),
                    "skipping terminal snapshot for fallback request id: its first write failed, capture_retry owns it"
                );
                return Ok((None, None));
            }
            let req = super::text_sanitize::request_with_clean_text(req);
            let req = req.as_ref();

            let prep = SnapshotPrep::compute(req)?;
            let assembled_streaming_body = self
                .assemble_streaming_body_if_needed(req, prep.request_id, &prep)
                .await?;
            let response_body_bytes: &[u8] = match &assembled_streaming_body {
                Some(bytes) => bytes.as_slice(),
                None => req.response_body.as_deref().unwrap_or(b""),
            };
            let title: Option<String> = self.resolve_title_for_snapshot(req).await?;

            let session_epoch = self.session_epoch();
            let mut tx = self.pool.begin().await?;

            let (session_db_id, project_id) = resolve_session_and_project_id(
                &mut *tx,
                &req.session_id,
                None,
                Some(req.message_count as i32),
            )
            .await?;

            update_request_status_and_error(&mut tx, &prep, req.completed_at).await?;
            let stream_blocks_gapped = check_stream_blocks_gapped(
                &mut tx,
                prep.request_id,
                req.expected_stream_block_count.max(0) as u32,
            )
            .await?;

            if let Some(rs) = prep.response_status {
                insert_terminal_response_row(
                    &mut tx,
                    &TerminalResponseInsert {
                        request_id: prep.request_id,
                        response_status: rs,
                        response_headers: &prep.response_headers,
                        response_body: response_body_bytes,
                        upstream_request_id: req.upstream_request_id.as_deref(),
                        upstream_message_id: req.upstream_message_id.as_deref(),
                        token_usage: &prep.token_usage,
                        stop_reason: req.stop_reason.as_deref(),
                        stop_details: req.stop_details.as_ref(),
                        message_extras: prep.message_extras.as_ref(),
                        completed_at: req.completed_at,
                        stream_blocks_gapped,
                    },
                )
                .await?;
            }

            let request_db_id = lookup_request_db_id(&mut tx, prep.request_id, req.created_at).await?;

            if !prep.diagnostics_dto.is_empty()
                && let Some(request_db_id) = request_db_id
            {
                insert_request_diagnostics(&mut tx, request_db_id, &prep.diagnostics).await?;
            }

            let disposition = turn_lifecycle::disposition_for_request(req);
            let closed_turn_id = match request_db_id {
                Some(request_db_id) => {
                    let ctx = TurnLifecycleCtx {
                        session_db_id,
                        project_id,
                        request_db_id,
                        disposition,
                        human_prompt: req.human_prompt.clone(),
                        is_streaming: req.is_streaming,
                    };
                    turn_lifecycle::apply_operator_turn_lifecycle(
                        &mut tx,
                        &ctx,
                        req.response_body.as_deref(),
                    )
                    .await?
                }
                None => None,
            };

            tx.commit().await?;
            self.remember_committed_session(session_epoch, &req.session_id, session_db_id);

            self.roll_session_summary_forward(session_db_id, req, &prep.status_str, title.as_deref())
                .await;

            tracing::debug!(
                request_id = %req.id,
                session_id = %req.session_id,
                project_id,
                title = title.as_deref().unwrap_or("(none)"),
                "persisted terminal request snapshot"
            );
            Ok((title, closed_turn_id))
        })
        .await
    }
}
