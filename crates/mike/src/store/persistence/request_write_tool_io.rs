use serde_json::Value as JsonValue;

use super::PersistError;
use super::capture_support::operation_for_tool;
use super::id_resolvers::{resolve_or_create_agent_id, resolve_tool_use_id};
use super::text_sanitize;
use crate::store::models::InterceptedRequest;
use crate::types::ToolCall;

struct ToolUseColumns {
    tool_ids: Vec<String>,
    ords: Vec<i32>,
    names: Vec<String>,
    inputs: Vec<Option<JsonValue>>,
    file_paths: Vec<Option<String>>,
    operations: Vec<Option<String>>,
}

impl ToolUseColumns {
    fn from_calls(calls: &[ToolCall]) -> Self {
        let mut cols = Self {
            tool_ids: Vec::with_capacity(calls.len()),
            ords: Vec::with_capacity(calls.len()),
            names: Vec::with_capacity(calls.len()),
            inputs: Vec::with_capacity(calls.len()),
            file_paths: Vec::with_capacity(calls.len()),
            operations: Vec::with_capacity(calls.len()),
        };
        for (ord, call) in calls.iter().enumerate() {
            cols.tool_ids.push(call.tool_id.clone());
            cols.ords.push(ord as i32);
            cols.names.push(call.name.clone());
            let mut input_val: Option<JsonValue> = if call.input_json.is_empty() {
                None
            } else {
                serde_json::from_str(&call.input_json).ok()
            };
            if let Some(value) = input_val.as_mut() {
                text_sanitize::clean_json(value);
            }
            let fp: Option<String> = input_val
                .as_ref()
                .and_then(|v| v.get("file_path"))
                .and_then(|v| v.as_str())
                .map(str::to_string);
            cols.file_paths.push(fp);
            cols.operations
                .push(operation_for_tool(&call.name).map(str::to_string));
            cols.inputs.push(input_val);
        }
        cols
    }
}

pub(crate) async fn write_tool_uses_and_dispatches(
    tx: &mut sqlx::PgConnection,
    req: &InterceptedRequest,
    request_db_id: i64,
    session_db_id: i64,
) -> Result<(), PersistError> {
    if req.tool_calls.is_empty() {
        return Ok(());
    }
    let cols = ToolUseColumns::from_calls(&req.tool_calls);
    sqlx::query!(
        "INSERT INTO tools.uses
            (wire_tool_use_id, request_id, ord, tool_name, input,
             file_path, operation, created_at, session_id)
         SELECT u.wire_tool_use_id, $1, u.ord, u.name, u.input,
                u.file_path, u.operation, $2, $9
         FROM UNNEST($3::text[], $4::int[], $5::text[], $6::jsonb[],
                     $7::text[], $8::text[])
              AS u(wire_tool_use_id, ord, name, input, file_path, operation)
         ON CONFLICT (wire_tool_use_id) DO NOTHING",
        request_db_id,
        req.created_at,
        &cols.tool_ids,
        &cols.ords,
        &cols.names,
        &cols.inputs as &[Option<JsonValue>],
        &cols.file_paths as &[Option<String>],
        &cols.operations as &[Option<String>],
        session_db_id,
    )
    .execute(&mut *tx)
    .await?;

    write_agent_dispatches(tx, req, &cols.inputs).await?;
    Ok(())
}

async fn write_agent_dispatches(
    tx: &mut sqlx::PgConnection,
    req: &InterceptedRequest,
    inputs: &[Option<JsonValue>],
) -> Result<(), PersistError> {
    for (call, input_value) in req.tool_calls.iter().zip(inputs.iter()) {
        if call.name != "Agent" {
            continue;
        }
        let name: String = input_value
            .as_ref()
            .and_then(|v| v.get("subagent_type"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| "general-purpose".to_string());
        let agent_id = resolve_or_create_agent_id(&mut *tx, &name).await?;
        let Some(tool_use_db_id) = resolve_tool_use_id(&mut *tx, &call.tool_id).await? else {
            continue;
        };
        sqlx::query!(
            "INSERT INTO agents.dispatches (tool_use_id, agent_id)
                     SELECT $1, $2
                     WHERE NOT EXISTS (
                         SELECT 1 FROM agents.dispatches WHERE tool_use_id = $1
                     )",
            tool_use_db_id,
            agent_id,
        )
        .execute(&mut *tx)
        .await?;
    }
    Ok(())
}

struct FailedStreamBlockColumns {
    seq_ords: Vec<i32>,
    block_types: Vec<String>,
    contents: Vec<String>,
    signatures: Vec<Option<String>>,
    wire_tool_use_ids: Vec<Option<String>>,
    shas: Vec<String>,
    byte_sizes: Vec<i32>,
}

impl FailedStreamBlockColumns {
    fn from_blocks(blocks: &[crate::store::models::FailedStreamBlock]) -> Self {
        let mut cols = Self {
            seq_ords: Vec::with_capacity(blocks.len()),
            block_types: Vec::with_capacity(blocks.len()),
            contents: Vec::with_capacity(blocks.len()),
            signatures: Vec::with_capacity(blocks.len()),
            wire_tool_use_ids: Vec::with_capacity(blocks.len()),
            shas: Vec::with_capacity(blocks.len()),
            byte_sizes: Vec::with_capacity(blocks.len()),
        };
        for b in blocks {
            cols.seq_ords.push(b.seq_ord);
            cols.block_types.push(b.block_type.clone());
            cols.contents
                .push(text_sanitize::clean_text(&b.content).into_owned());
            cols.signatures.push(b.signature.clone());
            cols.wire_tool_use_ids.push(b.wire_tool_use_id.clone());
            cols.shas.push(b.content_sha256.clone());
            cols.byte_sizes.push(b.content.len() as i32);
        }
        cols
    }
}

pub(crate) async fn retry_failed_stream_blocks(
    tx: &mut sqlx::PgConnection,
    req: &InterceptedRequest,
    request_db_id: i64,
    session_db_id: i64,
) -> Result<(), PersistError> {
    if req.failed_stream_blocks.is_empty() {
        return Ok(());
    }
    let cols = FailedStreamBlockColumns::from_blocks(&req.failed_stream_blocks);
    let savepoint = sqlx::Acquire::begin(&mut *tx).await?;
    insert_retried_stream_blocks(
        savepoint,
        request_db_id,
        session_db_id,
        &cols,
        req.failed_stream_blocks.len(),
    )
    .await
}

async fn insert_retried_stream_blocks(
    mut savepoint: sqlx::Transaction<'_, sqlx::Postgres>,
    request_db_id: i64,
    session_db_id: i64,
    cols: &FailedStreamBlockColumns,
    blocks_len: usize,
) -> Result<(), PersistError> {
    let retry_result = sqlx::query!(
        "INSERT INTO responses.stream_blocks
            (request_id, session_id, block_type_id, seq_ord, content,
             signature, wire_tool_use_id, content_sha256, byte_size)
         SELECT $1, $2, sbt.id, u.seq_ord, u.content,
                u.signature, u.wire_tool_use_id, u.content_sha256, u.byte_size
         FROM UNNEST($3::int[], $4::text[], $5::text[], $6::text[],
                     $7::text[], $8::text[], $9::int[])
              AS u(seq_ord, block_type, content, signature,
                   wire_tool_use_id, content_sha256, byte_size)
         JOIN responses.stream_block_types sbt ON sbt.code = u.block_type
         ON CONFLICT (request_id, seq_ord) DO NOTHING",
        request_db_id,
        session_db_id,
        &cols.seq_ords,
        &cols.block_types,
        &cols.contents,
        &cols.signatures as &[Option<String>],
        &cols.wire_tool_use_ids as &[Option<String>],
        &cols.shas,
        &cols.byte_sizes,
    )
    .execute(&mut *savepoint)
    .await;
    finish_stream_block_retry(savepoint, retry_result, request_db_id, blocks_len).await
}

async fn finish_stream_block_retry(
    savepoint: sqlx::Transaction<'_, sqlx::Postgres>,
    retry_result: Result<sqlx::postgres::PgQueryResult, sqlx::Error>,
    request_db_id: i64,
    blocks_len: usize,
) -> Result<(), PersistError> {
    match retry_result {
        Ok(_) => savepoint.commit().await?,
        Err(e) => {
            savepoint.rollback().await?;
            tracing::error!(
                request_id = request_db_id,
                blocks = blocks_len,
                error = %e,
                "stream_blocks terminal ENSURE retry failed — block(s) permanently unrecovered"
            );
        }
    }
    Ok(())
}

pub(crate) struct InsertedToolResult {
    pub(crate) wire_tool_use_id: String,
    pub(crate) tool_name: String,
}

pub(crate) async fn write_tool_results(
    tx: &mut sqlx::PgConnection,
    req: &InterceptedRequest,
    request_db_id: i64,
    session_db_id: i64,
) -> Result<Vec<InsertedToolResult>, PersistError> {
    if req.tool_results_in_body.is_empty() {
        return Ok(Vec::new());
    }
    let mut tr_req_ids: Vec<i64> = Vec::with_capacity(req.tool_results_in_body.len());
    let mut tr_created_ats: Vec<chrono::DateTime<chrono::Utc>> =
        Vec::with_capacity(req.tool_results_in_body.len());
    let mut tr_ords: Vec<i32> = Vec::with_capacity(req.tool_results_in_body.len());
    let mut tr_tool_use_ids: Vec<String> = Vec::with_capacity(req.tool_results_in_body.len());
    let mut tr_is_errors: Vec<bool> = Vec::with_capacity(req.tool_results_in_body.len());
    let mut tr_contents: Vec<JsonValue> = Vec::with_capacity(req.tool_results_in_body.len());
    let mut tr_shas: Vec<String> = Vec::with_capacity(req.tool_results_in_body.len());
    let mut tr_bytes: Vec<i64> = Vec::with_capacity(req.tool_results_in_body.len());
    for (ord, result) in req.tool_results_in_body.iter().enumerate() {
        tr_req_ids.push(request_db_id);
        tr_created_ats.push(req.created_at);
        tr_ords.push(ord as i32);
        tr_tool_use_ids.push(result.tool_use_id.clone());
        tr_is_errors.push(result.is_error);
        tr_contents.push(text_sanitize::cleaned_json(&result.content).into_owned());
        tr_shas.push(result.content_sha256.clone());
        tr_bytes.push(result.content_bytes as i64);
    }
    let inserted = sqlx::query!(
        "WITH ins AS (
             INSERT INTO tools.results
                (request_id, ord, tool_use_id,
                 is_error, content, content_sha256, content_bytes, created_at,
                 session_id)
             SELECT u.req_id, u.ord, tu.id,
                    u.is_err, u.content, u.sha, u.bytes, u.req_at,
                    $9::bigint
             FROM UNNEST(
                 $1::bigint[], $2::timestamptz[], $3::int[], $4::text[],
                 $5::bool[], $6::jsonb[], $7::text[], $8::bigint[]
             ) AS u(req_id, req_at, ord, tool_use_id, is_err, content, sha, bytes)
             JOIN tools.uses tu ON tu.wire_tool_use_id = u.tool_use_id
             ON CONFLICT (tool_use_id) DO NOTHING
             RETURNING tool_use_id
         )
         SELECT tu.wire_tool_use_id AS \"wire_tool_use_id!\", tu.tool_name AS \"tool_name!\"
         FROM ins JOIN tools.uses tu ON tu.id = ins.tool_use_id",
        &tr_req_ids,
        &tr_created_ats,
        &tr_ords,
        &tr_tool_use_ids,
        &tr_is_errors,
        &tr_contents,
        &tr_shas,
        &tr_bytes,
        session_db_id,
    )
    .fetch_all(&mut *tx)
    .await?;
    Ok(inserted
        .into_iter()
        .map(|row| InsertedToolResult {
            wire_tool_use_id: row.wire_tool_use_id,
            tool_name: row.tool_name,
        })
        .collect())
}
