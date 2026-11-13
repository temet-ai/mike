use serde_json::{Map, Value};
use sqlx::PgPool;

use super::error::PersistError;

#[derive(Debug)]
struct BlockRow {
    content: String,
    block_type: String,
    signature: Option<String>,
    wire_tool_use_id: Option<String>,
}

#[derive(Debug)]
struct ToolUseRow {
    wire_tool_use_id: String,
    tool_name: String,
    input: Option<Value>,
}

#[derive(Debug, Default)]
struct EnvelopeRow {
    model: Option<String>,
    upstream_message_id: Option<String>,
    stop_reason: Option<String>,
    token_usage: Option<Value>,
    message_extras: Option<Value>,
}

pub struct MessageEnvelope {
    pub upstream_message_id: Option<String>,
    pub model: Option<String>,
    pub stop_reason: Option<String>,
    pub token_usage: Value,
    pub message_extras: Option<Value>,
}

async fn build_content_blocks(pool: &PgPool, request_id: i64) -> Result<Vec<Value>, PersistError> {
    let blocks = sqlx::query_as!(
        BlockRow,
        r#"SELECT b.content,
                bt.code AS "block_type!",
                b.signature,
                b.wire_tool_use_id
         FROM responses.stream_blocks b
         JOIN responses.stream_block_types bt ON bt.id = b.block_type_id
         WHERE b.request_id = $1
         ORDER BY b.seq_ord ASC"#,
        request_id,
    )
    .fetch_all(pool)
    .await?;

    let tool_uses = sqlx::query_as!(
        ToolUseRow,
        r#"SELECT u.wire_tool_use_id,
                u.tool_name,
                u.input
         FROM tools.uses u
         WHERE u.request_id = $1
         ORDER BY u.ord ASC"#,
        request_id,
    )
    .fetch_all(pool)
    .await?;

    let tool_by_wire: std::collections::HashMap<&str, &ToolUseRow> = tool_uses
        .iter()
        .map(|r| (r.wire_tool_use_id.as_str(), r))
        .collect();

    let mut content: Vec<Value> = Vec::with_capacity(blocks.len());
    for block in &blocks {
        match block.block_type.as_str() {
            "text" => {
                let mut obj = Map::new();
                obj.insert("type".into(), Value::String("text".into()));
                obj.insert("text".into(), Value::String(block.content.clone()));
                content.push(Value::Object(obj));
            }
            "thinking" => {
                let mut obj = Map::new();
                obj.insert("type".into(), Value::String("thinking".into()));
                obj.insert("thinking".into(), Value::String(block.content.clone()));
                if let Some(sig) = &block.signature {
                    obj.insert("signature".into(), Value::String(sig.clone()));
                }
                content.push(Value::Object(obj));
            }
            "tool_use" => {
                let row = block
                    .wire_tool_use_id
                    .as_deref()
                    .and_then(|wid| tool_by_wire.get(wid).copied());
                let input_value = row
                    .and_then(|r| r.input.clone())
                    .unwrap_or_else(|| Value::Object(Map::new()));

                let mut obj = Map::new();
                obj.insert("type".into(), Value::String("tool_use".into()));
                if let Some(r) = row {
                    obj.insert("id".into(), Value::String(r.wire_tool_use_id.clone()));
                    obj.insert("name".into(), Value::String(r.tool_name.clone()));
                }
                obj.insert("input".into(), input_value);
                content.push(Value::Object(obj));
            }
            _ => {}
        }
    }

    Ok(content)
}

pub async fn assemble_message_with_envelope(
    pool: &PgPool,
    request_id: i64,
    envelope: MessageEnvelope,
) -> Result<Option<Value>, PersistError> {
    let content = build_content_blocks(pool, request_id).await?;
    if content.is_empty() {
        return Ok(None);
    }

    let mut msg = Map::new();
    msg.insert(
        "id".into(),
        match envelope.upstream_message_id {
            Some(id) => Value::String(id),
            None => Value::Null,
        },
    );
    msg.insert("type".into(), Value::String("message".into()));
    msg.insert("role".into(), Value::String("assistant".into()));
    msg.insert(
        "model".into(),
        match envelope.model {
            Some(m) => Value::String(m),
            None => Value::Null,
        },
    );
    msg.insert("content".into(), Value::Array(content));
    msg.insert(
        "stop_reason".into(),
        match envelope.stop_reason {
            Some(s) => Value::String(s),
            None => Value::Null,
        },
    );
    msg.insert("stop_sequence".into(), Value::Null);
    if let Some(Value::Object(extras)) = envelope.message_extras {
        for (key, value) in extras {
            msg.insert(key, value);
        }
    }
    msg.insert("usage".into(), envelope.token_usage);

    Ok(Some(Value::Object(msg)))
}

pub async fn assemble_message_json(
    pool: &PgPool,
    request_id: i64,
) -> Result<Option<Value>, PersistError> {
    let row = sqlx::query_as!(
        EnvelopeRow,
        r#"SELECT r.model AS "model?",
                resp.upstream_message_id AS "upstream_message_id?",
                sr.code           AS "stop_reason?",
                resp.token_usage AS "token_usage?",
                resp.message_extras AS "message_extras?"
         FROM requests.requests r
         LEFT JOIN responses.responses resp ON resp.request_id = r.id
         LEFT JOIN responses.stop_reasons sr ON sr.id = resp.stop_reason_id
         WHERE r.id = $1"#,
        request_id,
    )
    .fetch_optional(pool)
    .await?;

    let meta = row.unwrap_or_default();
    let envelope = MessageEnvelope {
        upstream_message_id: meta.upstream_message_id,
        model: meta.model,
        stop_reason: meta.stop_reason,
        token_usage: meta
            .token_usage
            .unwrap_or_else(|| Value::Object(Map::new())),
        message_extras: meta.message_extras,
    };

    assemble_message_with_envelope(pool, request_id, envelope).await
}
