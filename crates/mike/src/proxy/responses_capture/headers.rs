use axum::http::HeaderMap;
use serde_json::{Value, json};

use super::AuthRoute;
use crate::extract::headers::{captured_header_pairs, mask_header_value};

const ACCOUNT_HEADERS: &[&str] = &[
    "chatgpt-account-id",
    "openai-organization",
    "openai-project",
];

pub(super) fn auth_route(headers: &HeaderMap) -> AuthRoute {
    if headers.contains_key("chatgpt-account-id") {
        AuthRoute::Chatgpt
    } else if headers.contains_key("authorization") {
        AuthRoute::ApiKey
    } else {
        AuthRoute::None
    }
}

pub(super) fn is_websocket_upgrade(headers: &HeaderMap) -> bool {
    headers
        .get("upgrade")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
}

pub(super) fn captured(headers: &HeaderMap) -> Value {
    let pairs: Vec<Value> = captured_header_pairs(headers)
        .into_iter()
        .map(|(name, value)| {
            let stored = if ACCOUNT_HEADERS.contains(&name.as_str()) {
                mask_header_value(&value)
            } else {
                value
            };
            json!([name, stored])
        })
        .collect();
    Value::Array(pairs)
}

pub(super) fn text(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

pub(super) fn upstream_request_id(headers: &HeaderMap) -> Option<String> {
    ["x-request-id", "openai-request-id", "x-oai-request-id"]
        .iter()
        .find_map(|name| text(headers, name))
}
