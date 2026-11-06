use serde_json::Value;

use crate::extract::response::{
    extract_input_json_delta, extract_message_delta_fields, extract_message_start_fields,
    extract_signature_delta, extract_stop_details, extract_stop_reason, extract_text_delta,
    extract_thinking_delta, extract_tool_use_start, extract_usage_from_message_delta,
    extract_usage_from_message_start,
};
use crate::store::models::ProxyEvent;

use super::publish::{PublishCtx, publish};

pub(super) async fn handle_event(
    event_type: &str,
    data: &str,
    ctx: &mut PublishCtx<'_>,
    current_block_type: &mut Option<String>,
) {
    match event_type {
        "message_start" => handle_message_start(data, ctx).await,
        "content_block_start" => handle_content_block_start(data, ctx, current_block_type).await,
        "content_block_delta" => handle_content_block_delta(data, ctx).await,
        "content_block_stop" => handle_content_block_stop(ctx, current_block_type).await,
        "message_delta" => handle_message_delta(data, ctx).await,
        "message_stop" => log_message_stop(ctx),
        "ping" => log_ping(ctx),
        "error" => handle_error(data, ctx).await,
        other => log_unknown_event_type(ctx, other),
    }
}

fn log_message_stop(ctx: &PublishCtx<'_>) {
    tracing::debug!(request_id = %ctx.request_id, "message_stop received");
}

fn log_ping(ctx: &PublishCtx<'_>) {
    tracing::trace!(request_id = %ctx.request_id, "ping");
}

fn log_unknown_event_type(ctx: &PublishCtx<'_>, event_type: &str) {
    tracing::debug!(request_id = %ctx.request_id, event_type, "unknown SSE event type");
}

async fn publish_message_start_fields(json: &Value, ctx: &mut PublishCtx<'_>) {
    if let Some(fields) = extract_message_start_fields(json) {
        let id = ctx.request_id;
        publish(ctx, ProxyEvent::MessageFieldsReceived { id, fields }).await;
    }
}

async fn handle_message_start(data: &str, ctx: &mut PublishCtx<'_>) {
    let request_id = ctx.request_id;
    let Ok(json) = serde_json::from_str::<Value>(data) else {
        tracing::warn!(request_id = %request_id, "message_start: invalid JSON");
        return;
    };
    let model = json
        .pointer("/message/model")
        .and_then(|v| v.as_str())
        .map(String::from);
    let upstream_message_id = json
        .pointer("/message/id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);

    if let Some(usage) = extract_usage_from_message_start(&json) {
        publish(
            ctx,
            ProxyEvent::UsageReceived {
                id: request_id,
                usage,
            },
        )
        .await;
    }

    publish_message_start_fields(&json, ctx).await;

    if let Some(m) = model {
        publish(
            ctx,
            ProxyEvent::ModelConfirmed {
                id: request_id,
                model: m,
            },
        )
        .await;
    }

    if let Some(mid) = upstream_message_id {
        publish(
            ctx,
            ProxyEvent::UpstreamMessageIdReceived {
                id: request_id,
                upstream_message_id: mid,
            },
        )
        .await;
    }
}

async fn handle_content_block_start(
    data: &str,
    ctx: &mut PublishCtx<'_>,
    current_block_type: &mut Option<String>,
) {
    let request_id = ctx.request_id;
    let Ok(json) = serde_json::from_str::<Value>(data) else {
        tracing::warn!(request_id = %request_id, "content_block_start: invalid JSON");
        return;
    };
    let block_type = json
        .pointer("/content_block/type")
        .and_then(|v| v.as_str())
        .map(String::from);
    *current_block_type = block_type.clone();

    if block_type.as_deref() == Some("tool_use")
        && let Some((tid, tname)) = extract_tool_use_start(&json)
    {
        publish(
            ctx,
            ProxyEvent::ToolUseStart {
                id: request_id,
                tool_id: tid,
                tool_name: tname,
            },
        )
        .await;
    }
}

async fn handle_content_block_delta(data: &str, ctx: &mut PublishCtx<'_>) {
    let request_id = ctx.request_id;
    let Ok(json) = serde_json::from_str::<Value>(data) else {
        tracing::warn!(request_id = %request_id, "content_block_delta: invalid JSON");
        return;
    };
    match json.pointer("/delta/type").and_then(|v| v.as_str()) {
        Some("text_delta") => publish_text_delta(&json, ctx).await,
        Some("thinking_delta") => publish_thinking_delta(&json, ctx).await,
        Some("input_json_delta") => publish_tool_input_delta(&json, ctx).await,
        Some("signature_delta") => publish_signature_delta(&json, ctx).await,
        _ => {}
    }
}

async fn publish_text_delta(json: &Value, ctx: &mut PublishCtx<'_>) {
    let request_id = ctx.request_id;
    if let Some(text) = extract_text_delta(json) {
        publish(
            ctx,
            ProxyEvent::TextDelta {
                id: request_id,
                text,
            },
        )
        .await;
    }
}

async fn publish_thinking_delta(json: &Value, ctx: &mut PublishCtx<'_>) {
    let request_id = ctx.request_id;
    if let Some(thinking) = extract_thinking_delta(json) {
        publish(
            ctx,
            ProxyEvent::ThinkingDelta {
                id: request_id,
                thinking,
            },
        )
        .await;
    }
}

async fn publish_tool_input_delta(json: &Value, ctx: &mut PublishCtx<'_>) {
    let request_id = ctx.request_id;
    if let Some(partial_json) = extract_input_json_delta(json) {
        publish(
            ctx,
            ProxyEvent::ToolInputDelta {
                id: request_id,
                partial_json,
            },
        )
        .await;
    }
}

async fn publish_signature_delta(json: &Value, ctx: &mut PublishCtx<'_>) {
    let request_id = ctx.request_id;
    if let Some(signature) = extract_signature_delta(json) {
        publish(
            ctx,
            ProxyEvent::SignatureDelta {
                id: request_id,
                signature,
            },
        )
        .await;
    }
}

async fn handle_content_block_stop(
    ctx: &mut PublishCtx<'_>,
    current_block_type: &mut Option<String>,
) {
    let request_id = ctx.request_id;
    if current_block_type.as_deref() == Some("tool_use") {
        publish(ctx, ProxyEvent::ToolUseEnd { id: request_id }).await;
    }
    *current_block_type = None;
}

async fn handle_message_delta(data: &str, ctx: &mut PublishCtx<'_>) {
    let request_id = ctx.request_id;
    let Ok(json) = serde_json::from_str::<Value>(data) else {
        tracing::warn!(request_id = %request_id, "message_delta: invalid JSON");
        return;
    };
    let stop_reason = extract_stop_reason(&json);
    let stop_details = extract_stop_details(&json);
    let usage = extract_usage_from_message_delta(&json);
    if let Some(fields) = extract_message_delta_fields(&json) {
        publish(
            ctx,
            ProxyEvent::MessageFieldsReceived {
                id: request_id,
                fields,
            },
        )
        .await;
    }
    publish(
        ctx,
        ProxyEvent::ResponseComplete {
            id: request_id,
            stop_reason,
            stop_details,
            usage,
        },
    )
    .await;
}

async fn handle_error(data: &str, ctx: &mut PublishCtx<'_>) {
    let request_id = ctx.request_id;
    let error_msg = match serde_json::from_str::<Value>(data) {
        Ok(json) => json
            .pointer("/error/message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error")
            .to_string(),
        Err(_) => "unparseable error event".to_string(),
    };
    publish(
        ctx,
        ProxyEvent::ResponseError {
            id: request_id,
            error: error_msg,
        },
    )
    .await;
}
