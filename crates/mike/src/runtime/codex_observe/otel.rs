use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::{Value, json};

type Attrs = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ToolResult {
    pub thread_id: Option<String>,
    pub call_id: String,
    pub tool_name: String,
    pub namespace: Option<String>,
    pub mcp_server: Option<String>,
    pub duration_ms: Option<i64>,
    pub success: Option<bool>,
    pub process_session: Option<String>,
    pub exit_code: Option<i32>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RequestAttempt {
    pub thread_id: Option<String>,
    pub event: String,
    pub endpoint: Option<String>,
    pub attempt: Option<i32>,
    pub http_status: Option<i32>,
    pub success: Option<bool>,
    pub error: Option<String>,
    pub duration_ms: Option<i64>,
    pub usage: Option<Value>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TelemetryFact {
    Conversation {
        thread_id: String,
        model: Option<String>,
        codex_version: Option<String>,
        originator: Option<String>,
        at: DateTime<Utc>,
    },
    ToolDecision {
        thread_id: Option<String>,
        call_id: String,
        tool_name: String,
        namespace: Option<String>,
        at: DateTime<Utc>,
    },
    ToolResult(ToolResult),
    Request(RequestAttempt),
    Spawn {
        parent_thread_id: String,
        child_thread_id: String,
        at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HookRun {
    pub trace_id: String,
    pub span_id: String,
    pub event_name: String,
    pub outcome: String,
    pub execution_mode: Option<String>,
    pub timeout_sec: Option<i32>,
    pub started_at: DateTime<Utc>,
    pub duration_ms: i64,
}

fn any_value(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    let (_, inner) = object.iter().next()?;
    match inner {
        Value::String(text) => Some(text.clone()),
        Value::Null => None,
        other => Some(other.to_string()),
    }
}

fn attrs(record: &Value) -> Attrs {
    record
        .get("attributes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|attr| {
            Some((
                attr.get("key")?.as_str()?.to_owned(),
                any_value(attr.get("value")?)?,
            ))
        })
        .collect()
}

fn nanos(text: Option<&Value>) -> Option<DateTime<Utc>> {
    let raw = text?.as_str()?.parse::<i64>().ok().filter(|n| *n > 0)?;
    Some(DateTime::from_timestamp_nanos(raw))
}

fn record_time(record: &Value, attrs: &Attrs) -> DateTime<Utc> {
    attrs
        .get("event.timestamp")
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .map(|time| time.with_timezone(&Utc))
        .or_else(|| nanos(record.get("timeUnixNano")))
        .or_else(|| nanos(record.get("observedTimeUnixNano")))
        .unwrap_or_else(Utc::now)
}

fn int<T: std::str::FromStr>(attrs: &Attrs, key: &str) -> Option<T> {
    attrs.get(key).and_then(|value| value.parse().ok())
}

fn flag(attrs: &Attrs, key: &str) -> Option<bool> {
    attrs
        .get(key)
        .map(|value| value.eq_ignore_ascii_case("true"))
}

fn text(attrs: &Attrs, key: &str) -> Option<String> {
    attrs.get(key).filter(|value| !value.is_empty()).cloned()
}

fn after<'a>(haystack: &'a str, marker: &str) -> Option<&'a str> {
    let start = haystack.find(marker)? + marker.len();
    let rest = &haystack[start..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '-'))
        .unwrap_or(rest.len());
    Some(&rest[..end]).filter(|digits| !digits.is_empty())
}

fn poll_session(arguments: Option<&str>) -> Option<String> {
    let parsed: Value = serde_json::from_str(arguments?).ok()?;
    parsed.get("session_id").map(|id| match id {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    })
}

fn tool_result(attrs: &Attrs, at: DateTime<Utc>) -> Option<TelemetryFact> {
    let output = attrs.get("output").map(String::as_str).unwrap_or_default();
    let arguments = text(attrs, "arguments");
    let process_session = after(output, "Process running with session ID ")
        .map(str::to_owned)
        .or_else(|| poll_session(arguments.as_deref()));
    Some(TelemetryFact::ToolResult(ToolResult {
        thread_id: text(attrs, "conversation.id"),
        call_id: text(attrs, "call_id")?,
        tool_name: text(attrs, "tool_name")?,
        namespace: text(attrs, "tool_namespace"),
        mcp_server: text(attrs, "mcp_server"),
        duration_ms: int(attrs, "duration_ms"),
        success: flag(attrs, "success"),
        process_session,
        exit_code: after(output, "Process exited with code ").and_then(|code| code.parse().ok()),
        at,
    }))
}

fn usage(attrs: &Attrs) -> Option<Value> {
    let keys = [
        ("input_tokens", "input_token_count"),
        ("output_tokens", "output_token_count"),
        ("cached_input_tokens", "cached_token_count"),
        ("cache_write_input_tokens", "cache_write_token_count"),
        ("reasoning_output_tokens", "reasoning_token_count"),
        ("tool_tokens", "tool_token_count"),
    ];
    let present: serde_json::Map<String, Value> = keys
        .iter()
        .filter_map(|(name, key)| Some(((*name).to_owned(), json!(int::<i64>(attrs, key)?))))
        .collect();
    (!present.is_empty()).then_some(Value::Object(present))
}

fn request(event: &str, attrs: &Attrs, at: DateTime<Utc>) -> TelemetryFact {
    TelemetryFact::Request(RequestAttempt {
        thread_id: text(attrs, "conversation.id"),
        event: event.to_owned(),
        endpoint: text(attrs, "endpoint").or_else(|| text(attrs, "event.kind")),
        attempt: int(attrs, "attempt"),
        http_status: int(attrs, "http.response.status_code"),
        success: flag(attrs, "success"),
        error: text(attrs, "error.message").or_else(|| text(attrs, "error")),
        duration_ms: int(attrs, "duration_ms"),
        usage: usage(attrs),
        at,
    })
}

fn log_fact(record: &Value) -> Option<TelemetryFact> {
    let attrs = attrs(record);
    let at = record_time(record, &attrs);
    let event = attrs.get("event.name")?.as_str();
    match event {
        "codex.conversation_starts" => Some(TelemetryFact::Conversation {
            thread_id: text(&attrs, "conversation.id")?,
            model: text(&attrs, "model"),
            codex_version: text(&attrs, "app.version"),
            originator: text(&attrs, "originator"),
            at,
        }),
        "codex.tool_decision" => Some(TelemetryFact::ToolDecision {
            thread_id: text(&attrs, "conversation.id"),
            call_id: text(&attrs, "call_id")?,
            tool_name: text(&attrs, "tool_name")?,
            namespace: text(&attrs, "tool_namespace"),
            at,
        }),
        "codex.tool_result" => tool_result(&attrs, at),
        "codex.api_request" | "codex.websocket_request" | "codex.websocket_connect" => {
            Some(request(event, &attrs, at))
        }
        "codex.sse_event"
            if attrs.get("event.kind").map(String::as_str) == Some("response.completed") =>
        {
            Some(request(event, &attrs, at))
        }
        "codex.agent_communication" if attrs.get("kind").map(String::as_str) == Some("spawn") => {
            Some(TelemetryFact::Spawn {
                parent_thread_id: text(&attrs, "sender_thread_id")?,
                child_thread_id: text(&attrs, "receiver_thread_id")?,
                at,
            })
        }
        _ => None,
    }
}

fn nested<'a>(body: &'a Value, keys: [&'static str; 3]) -> impl Iterator<Item = &'a Value> + 'a {
    let [outer, middle, inner] = keys;
    body.get(outer)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(move |group| {
            group
                .get(middle)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .flat_map(move |scope| {
            scope
                .get(inner)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
}

pub(crate) fn log_facts(body: &Value) -> Vec<TelemetryFact> {
    nested(body, ["resourceLogs", "scopeLogs", "logRecords"])
        .filter_map(log_fact)
        .collect()
}

pub(crate) fn resource_version(body: &Value) -> Option<String> {
    ["resourceLogs", "resourceSpans", "resourceMetrics"]
        .iter()
        .filter_map(|key| body.get(*key).and_then(Value::as_array))
        .flatten()
        .find_map(|group| {
            attrs(group.get("resource")?)
                .get("service.version")
                .cloned()
        })
}

fn hook_run(span: &Value) -> Option<HookRun> {
    if span.get("name")?.as_str()? != "codex.hooks.command" {
        return None;
    }
    let attrs = attrs(span);
    let started_at = nanos(span.get("startTimeUnixNano"))?;
    let ended_at = nanos(span.get("endTimeUnixNano")).unwrap_or(started_at);
    Some(HookRun {
        trace_id: span.get("traceId")?.as_str()?.to_owned(),
        span_id: span.get("spanId")?.as_str()?.to_owned(),
        event_name: text(&attrs, "hook.event_name")?,
        outcome: text(&attrs, "hook.command_outcome").unwrap_or_else(|| "unknown".into()),
        execution_mode: text(&attrs, "hook.execution_mode"),
        timeout_sec: int(&attrs, "hook.timeout_sec"),
        started_at,
        duration_ms: (ended_at - started_at).num_milliseconds().max(0),
    })
}

pub(crate) fn hook_runs(body: &Value) -> Vec<HookRun> {
    nested(body, ["resourceSpans", "scopeSpans", "spans"])
        .filter_map(hook_run)
        .collect()
}
