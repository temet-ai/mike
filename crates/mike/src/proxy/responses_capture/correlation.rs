use axum::http::HeaderMap;
use serde_json::{Map, Value, json};

pub(super) const TURN_METADATA_HEADER: &str = "x-codex-turn-metadata";

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Correlation {
    pub(crate) session_uuid: Option<uuid::Uuid>,
    pub(crate) provider_session_id: Option<String>,
    pub(crate) provider_thread_id: Option<String>,
    pub(crate) provider_turn_id: Option<String>,
    pub(crate) request_kind: Option<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) evidence: Value,
}

pub(crate) fn correlate(headers: &HeaderMap, body: &[u8]) -> Correlation {
    let mut evidence = Map::new();
    let metadata = turn_metadata(headers, &mut evidence);
    let provider_session_id = header_or_metadata(
        headers,
        "session-id",
        metadata.as_ref(),
        "session_id",
        &mut evidence,
    );
    let provider_thread_id = header_or_metadata(
        headers,
        "thread-id",
        metadata.as_ref(),
        "thread_id",
        &mut evidence,
    );
    let provider_turn_id = metadata_text(metadata.as_ref(), "turn_id");
    record(
        &mut evidence,
        "turn_id",
        provider_turn_id.as_deref(),
        TURN_METADATA_HEADER,
    );
    let request_kind = metadata_text(metadata.as_ref(), "request_kind");
    record(
        &mut evidence,
        "request_kind",
        request_kind.as_deref(),
        TURN_METADATA_HEADER,
    );
    let cwd = cwd_from(metadata.as_ref(), body, &mut evidence);
    let session_uuid = provider_session_id
        .as_deref()
        .and_then(|id| uuid::Uuid::parse_str(id).ok());
    if provider_session_id.is_some() && session_uuid.is_none() {
        evidence.insert(
            "session_uuid".into(),
            json!({"unknown": "session id is not a UUID; no mike session is linked"}),
        );
    }
    Correlation {
        session_uuid,
        provider_session_id,
        provider_thread_id,
        provider_turn_id,
        request_kind,
        cwd,
        evidence: Value::Object(evidence),
    }
}

fn turn_metadata(headers: &HeaderMap, evidence: &mut Map<String, Value>) -> Option<Value> {
    let raw = headers.get(TURN_METADATA_HEADER)?.to_str().ok()?;
    match serde_json::from_str::<Value>(raw) {
        Ok(value) if value.is_object() => Some(value),
        _ => {
            evidence.insert(
                "turn_metadata".into(),
                json!({"unknown": "header present but not a JSON object"}),
            );
            None
        }
    }
}

fn header_text(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

fn metadata_text(metadata: Option<&Value>, key: &str) -> Option<String> {
    metadata?
        .get(key)?
        .as_str()
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

fn header_or_metadata(
    headers: &HeaderMap,
    header: &str,
    metadata: Option<&Value>,
    key: &str,
    evidence: &mut Map<String, Value>,
) -> Option<String> {
    if let Some(value) = header_text(headers, header) {
        evidence.insert(key.into(), json!({"from": format!("header:{header}")}));
        return Some(value);
    }
    let value = metadata_text(metadata, key);
    record(evidence, key, value.as_deref(), TURN_METADATA_HEADER);
    value
}

fn record(evidence: &mut Map<String, Value>, key: &str, value: Option<&str>, source: &str) {
    let entry = match value {
        Some(_) => json!({"from": format!("header:{source}")}),
        None => json!({"unknown": "no evidence in this request"}),
    };
    evidence.insert(key.into(), entry);
}

fn cwd_from(
    metadata: Option<&Value>,
    body: &[u8],
    evidence: &mut Map<String, Value>,
) -> Option<String> {
    let workspaces = metadata
        .and_then(|m| m.get("workspaces"))
        .and_then(Value::as_object);
    if let Some(cwd) = workspaces.and_then(|w| (w.len() == 1).then(|| w.keys().next()).flatten()) {
        evidence.insert(
            "cwd".into(),
            json!({"from": format!("header:{TURN_METADATA_HEADER}.workspaces")}),
        );
        return Some(cwd.clone());
    }
    if let Some(cwd) = cwd_in_body(body) {
        evidence.insert(
            "cwd".into(),
            json!({"from": "body:<environment_context><cwd>"}),
        );
        return Some(cwd);
    }
    evidence.insert(
        "cwd".into(),
        json!({"unknown": "no single workspace in the turn metadata and no <cwd> in the body"}),
    );
    None
}

fn cwd_in_body(body: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(body).ok()?;
    let start = text.find("<cwd>")? + "<cwd>".len();
    let end = text[start..].find("</cwd>")? + start;
    let cwd = text[start..end].trim();
    (!cwd.is_empty() && !cwd.contains('\\')).then(|| cwd.to_string())
}

#[cfg(test)]
#[path = "correlation_tests.rs"]
mod correlation_tests;
