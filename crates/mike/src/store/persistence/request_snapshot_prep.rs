use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::PersistError;
use crate::store::models::{Diagnostic, DiagnosticCategory, InterceptedRequest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DiagnosticPersistDto {
    category: DiagnosticCategory,
    analyzer: String,
    advice: String,
    matched_text: String,
    match_range: (usize, usize),
    other_hits: usize,
    created_at: String,
}

impl From<&Diagnostic> for DiagnosticPersistDto {
    fn from(d: &Diagnostic) -> Self {
        Self {
            category: d.category.clone(),
            analyzer: d.analyzer.clone(),
            advice: d.advice.clone(),
            matched_text: d.matched_text.clone(),
            match_range: d.match_range,
            other_hits: d.other_hits,
            created_at: d.created_at.to_rfc3339(),
        }
    }
}

pub(crate) fn extract_title_for_snapshot(req: &InterceptedRequest) -> Option<String> {
    if req.turn_origin != "title_sidecar" {
        return None;
    }
    if !matches!(req.status, crate::store::models::RequestStatus::Complete) {
        return None;
    }
    let body = req.response_body.as_deref()?;
    let text = crate::extract::response::extract_accumulated_text_from_body(body);
    if text.is_empty() {
        return None;
    }
    let parsed: serde_json::Value = serde_json::from_str(&text).ok()?;
    let title = parsed.get("title")?.as_str()?.trim();
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

pub(crate) struct SnapshotPrep {
    pub(crate) response_headers: JsonValue,
    pub(crate) diagnostics_dto: Vec<DiagnosticPersistDto>,
    pub(crate) diagnostics: JsonValue,
    pub(crate) token_usage: JsonValue,
    pub(crate) message_extras: Option<JsonValue>,
    pub(crate) status_str: String,
    pub(crate) status_code: &'static str,
    pub(crate) status_error_message: Option<String>,
    pub(crate) response_status: Option<i16>,
    pub(crate) request_id: i64,
}

impl SnapshotPrep {
    pub(crate) fn compute(req: &InterceptedRequest) -> Result<Self, PersistError> {
        let response_headers = match &req.response_headers {
            Some(h) => serde_json::to_value(h)?,
            None => JsonValue::Array(Vec::new()),
        };
        let diagnostics_dto: Vec<DiagnosticPersistDto> = req
            .diagnostics
            .iter()
            .map(DiagnosticPersistDto::from)
            .collect();
        let diagnostics = serde_json::to_value(&diagnostics_dto)?;
        let mut token_usage = match &req.token_usage {
            Some(t) => serde_json::to_value(t)?,
            None => JsonValue::Object(serde_json::Map::new()),
        };
        let mut message_extras = None;
        if let Some(fields) = &req.message_fields {
            let (wire_usage, rest) = crate::extract::response::split_message_fields(fields);
            if let (Some(JsonValue::Object(wire)), Some(base)) =
                (wire_usage, token_usage.as_object_mut())
            {
                for (k, v) in wire {
                    base.insert(k.clone(), v.clone());
                }
            }
            message_extras = rest;
        }
        let status_str = req.status.to_string();
        let (status_code, status_error_message): (&str, Option<String>) = match &req.status {
            crate::store::models::RequestStatus::Pending => ("pending", None),
            crate::store::models::RequestStatus::Streaming => ("streaming", None),
            crate::store::models::RequestStatus::Complete => ("complete", None),
            crate::store::models::RequestStatus::Error(msg) => ("error", Some(msg.clone())),
        };
        let response_status = req.response_status.map(|s| s as i16);
        let request_id = req.id.as_u64() as i64;
        Ok(Self {
            response_headers,
            diagnostics_dto,
            diagnostics,
            token_usage,
            message_extras,
            status_str,
            status_code,
            status_error_message,
            response_status,
            request_id,
        })
    }
}
