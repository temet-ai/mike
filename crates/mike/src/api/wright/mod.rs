use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

pub(crate) mod checks;
pub(crate) mod problems;

#[cfg(test)]
mod checks_tests;

pub(crate) const PAGE_MAX_LIMIT: i64 = 500;

pub(crate) fn refuse(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"error": message}))).into_response()
}

pub(crate) fn bad_request(message: String) -> Response {
    refuse(StatusCode::BAD_REQUEST, &message)
}

pub(crate) fn page_limit(value: Option<i64>, default: i64, label: &str) -> Result<i64, String> {
    let value = value.unwrap_or(default);
    if (1..=PAGE_MAX_LIMIT).contains(&value) {
        return Ok(value);
    }
    Err(format!(
        "{label} is {value}; it must be between 1 and {PAGE_MAX_LIMIT}"
    ))
}

pub(crate) fn page_offset(value: Option<i64>, label: &str) -> Result<i64, String> {
    let value = value.unwrap_or(0);
    if value >= 0 {
        return Ok(value);
    }
    Err(format!("{label} is {value}; it must be 0 or more"))
}
