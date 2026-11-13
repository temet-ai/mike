use sqlx::PgPool;

use super::row::{self, PersistedRequestRow};
use crate::store::models::InterceptedRequest;
use crate::store::persistence::PersistError;
use crate::store::persistence::messages;
use crate::store::persistence::reconstruct;
use crate::types::ToolCall;

#[derive(Debug)]
struct ToolUseRow {
    tool_id: String,
    tool_name: String,
    input: Option<serde_json::Value>,
}

impl From<ToolUseRow> for ToolCall {
    fn from(r: ToolUseRow) -> Self {
        Self {
            tool_id: r.tool_id,
            name: r.tool_name,
            input_json: r.input.map(|v| v.to_string()).unwrap_or_default(),
            complete: true,
        }
    }
}

async fn fetch_invoked_tool_calls(
    pool: &PgPool,
    request_id: i64,
) -> Result<Vec<ToolCall>, PersistError> {
    let invoked = sqlx::query_as!(
        ToolUseRow,
        "SELECT tu.wire_tool_use_id AS tool_id, tu.tool_name, tu.input
           FROM tools.uses tu
          WHERE tu.request_id = $1
          ORDER BY tu.ord",
        request_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(invoked.into_iter().map(Into::into).collect())
}

async fn fetch_declared_tool_names(
    pool: &PgPool,
    request_id: i64,
) -> Result<Vec<String>, PersistError> {
    let declared = sqlx::query_scalar!(
        "SELECT rt.tool_name
           FROM requests.tools rt
          WHERE rt.request_id = $1
          ORDER BY rt.ord",
        request_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(declared)
}

pub async fn fetch_request_by_id(
    pool: &PgPool,
    request_id: u64,
) -> Result<Option<InterceptedRequest>, PersistError> {
    let id_i64 = i64::try_from(request_id).unwrap_or(i64::MAX);
    let row = sqlx::query_file_as!(
        PersistedRequestRow,
        "src/store/persistence/rehydrate/request_by_id.sql",
        id_i64,
    )
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };

    let tool_calls = fetch_invoked_tool_calls(pool, row.id).await?;
    let tool_names = fetch_declared_tool_names(pool, row.id).await?;

    Ok(Some(
        row::row_to_intercepted(row, tool_names, tool_calls).await?,
    ))
}

pub async fn fetch_request_body_by_id(
    pool: &PgPool,
    request_id: u64,
) -> Result<Option<reconstruct::ServedBody>, PersistError> {
    let id_i64 = i64::try_from(request_id).unwrap_or(i64::MAX);
    let row = sqlx::query!(
        "SELECT request_body, request_envelope
         FROM requests.requests
         WHERE id = $1",
        id_i64,
    )
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    reconstruct::rehydrate_body(
        pool,
        id_i64,
        row.request_body.map(bytes::Bytes::from),
        row.request_envelope.map(bytes::Bytes::from),
    )
    .await
}

pub async fn fetch_request_sent_body_by_id(
    pool: &PgPool,
    request_id: u64,
) -> Result<Option<bytes::Bytes>, PersistError> {
    let id_i64 = i64::try_from(request_id).unwrap_or(i64::MAX);
    let row = sqlx::query!(
        "SELECT request_body, filtered_request_body, request_envelope
         FROM requests.requests
         WHERE id = $1",
        id_i64,
    )
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    if let Some(legacy) = row.filtered_request_body {
        return Ok(Some(bytes::Bytes::from(legacy)));
    }
    let Some(served) = reconstruct::rehydrate_body(
        pool,
        id_i64,
        row.request_body.map(bytes::Bytes::from),
        row.request_envelope.map(bytes::Bytes::from),
    )
    .await?
    else {
        return Ok(None);
    };
    let request_body = served.bytes;
    let mut patches: Vec<messages::SentPatchRow> = sqlx::query_as!(
        messages::SentPatchRow,
        r#"SELECT sent_patch_start AS "sent_patch_start!",
                  sent_patch_end   AS "sent_patch_end!",
                  sent_patch_bytes AS "sent_patch_bytes!"
             FROM requests.deltas
            WHERE request_id = $1
              AND sent_patch_start IS NOT NULL"#,
        id_i64,
    )
    .fetch_all(pool)
    .await?;
    let envelope: Option<messages::SentPatchRow> = sqlx::query_as!(
        messages::SentPatchRow,
        r#"SELECT sent_patch_start AS "sent_patch_start!",
                  sent_patch_end   AS "sent_patch_end!",
                  sent_patch_bytes AS "sent_patch_bytes!"
             FROM requests.requests
            WHERE id = $1
              AND sent_patch_start IS NOT NULL"#,
        id_i64,
    )
    .fetch_optional(pool)
    .await?;
    patches.extend(envelope);
    if patches.is_empty() {
        return Ok(Some(request_body));
    }
    let reconstructed = messages::apply_sent_patches(&request_body, patches)?;
    Ok(Some(bytes::Bytes::from(reconstructed)))
}

pub async fn fetch_predecessor_request_id(
    pool: &PgPool,
    session_db_id: i64,
    created_at: chrono::DateTime<chrono::Utc>,
    agent_instance_id: Option<i64>,
) -> Result<Option<i64>, PersistError> {
    match agent_instance_id {
        Some(instance_id) => {
            let row = sqlx::query_scalar!(
                "SELECT r.id
                 FROM   requests.requests r
                 JOIN   requests.turn_origins tor ON tor.id = r.turn_origin_id AND tor.code = 'subagent'
                 WHERE  r.session_id         = $1
                   AND  r.agent_instance_id  = $3
                   AND  r.created_at         < $2
                 ORDER  BY r.created_at DESC, r.id DESC
                 LIMIT  1",
                session_db_id,
                created_at,
                instance_id,
            )
            .fetch_optional(pool)
            .await?;
            Ok(row)
        }
        None => {
            let row = sqlx::query_scalar!(
                "SELECT r.id
                 FROM   requests.requests r
                 JOIN   requests.turn_origins tor ON tor.id = r.turn_origin_id AND tor.code IN ('primary', 'primary_first')
                 WHERE  r.session_id         = $1
                   AND  r.agent_instance_id IS NULL
                   AND  r.created_at         < $2
                 ORDER  BY r.created_at DESC, r.id DESC
                 LIMIT  1",
                session_db_id,
                created_at,
            )
            .fetch_optional(pool)
            .await?;
            Ok(row)
        }
    }
}

pub async fn fetch_request_number(
    pool: &PgPool,
    request_id: u64,
) -> Result<Option<i64>, PersistError> {
    let row = sqlx::query_scalar!(
        "SELECT request_number FROM requests.requests WHERE id = $1",
        request_id as i64,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.flatten())
}

pub async fn fetch_response_body_by_id(
    pool: &PgPool,
    request_id: u64,
) -> Result<Option<bytes::Bytes>, PersistError> {
    let id_i64 = i64::try_from(request_id).unwrap_or(i64::MAX);
    let row = sqlx::query_scalar!(
        "SELECT response_body
         FROM responses.responses
         WHERE request_id = $1",
        id_i64,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|body| {
        if body.is_empty() {
            None
        } else {
            Some(bytes::Bytes::from(body))
        }
    }))
}

pub async fn count_session_requests(
    pool: &PgPool,
    session_db_id: i64,
) -> Result<usize, PersistError> {
    let count = sqlx::query_scalar!(
        r#"SELECT COUNT(*)::bigint AS "count!"
           FROM requests.requests r
           WHERE r.session_id = $1"#,
        session_db_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(count.max(0) as usize)
}
