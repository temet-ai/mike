use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

pub(crate) mod checks;
pub(crate) mod conversation;
pub(crate) mod fix_batches;
pub(crate) mod followups;
pub(crate) mod investigations;
pub(crate) mod problems;
pub(crate) mod relay;
pub(crate) mod remedies;
pub(crate) mod repairs;

#[cfg(test)]
mod checks_tests;
#[cfg(test)]
mod conversation_post_tests;
#[cfg(test)]
mod conversation_tests;
#[cfg(test)]
mod conversation_undo_tests;
#[cfg(test)]
mod followups_tests;
#[cfg(test)]
mod investigations_tests;
#[cfg(test)]
mod relay_tests;
#[cfg(test)]
mod remedies_tests;

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

pub(crate) fn page_limit_up_to(
    value: Option<i64>,
    default: i64,
    max: i64,
    label: &str,
) -> Result<i64, String> {
    let value = value.unwrap_or(default);
    if (1..=max).contains(&value) {
        return Ok(value);
    }
    Err(format!(
        "{label} is {value}; it must be between 1 and {max}"
    ))
}

pub(crate) fn page_offset(value: Option<i64>, label: &str) -> Result<i64, String> {
    let value = value.unwrap_or(0);
    if value >= 0 {
        return Ok(value);
    }
    Err(format!("{label} is {value}; it must be 0 or more"))
}
