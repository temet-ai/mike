use axum::http::HeaderMap;
use axum::http::header::{ORIGIN, USER_AGENT};

use crate::change_log::{ChangeSource, ChangeVia};

fn header_text(headers: &HeaderMap, name: impl axum::http::header::AsHeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn change_reason(headers: &HeaderMap) -> Option<String> {
    let encoded = header_text(headers, "x-change-reason")?;
    let query = format!("reason={}", encoded.replace('+', "%2B"));
    url::form_urlencoded::parse(query.as_bytes())
        .next()
        .map(|(_, value)| value.into_owned())
}

pub(crate) fn change_source_from_headers(headers: &HeaderMap) -> ChangeSource {
    let via = if headers.contains_key(ORIGIN) {
        ChangeVia::Spa
    } else {
        ChangeVia::Api
    };
    ChangeSource {
        via,
        actor: header_text(headers, USER_AGENT).or_else(|| header_text(headers, ORIGIN)),
        request_id: header_text(headers, "x-request-id"),
        detail: serde_json::json!({"reason": change_reason(headers)}),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn origin_header_marks_a_spa_change() {
        let mut headers = HeaderMap::new();
        headers.insert(ORIGIN, HeaderValue::from_static("http://localhost:9990"));
        headers.insert("x-request-id", HeaderValue::from_static("req-1"));
        let source = change_source_from_headers(&headers);
        assert_eq!(source.via, ChangeVia::Spa);
        assert_eq!(source.request_id.as_deref(), Some("req-1"));
    }

    #[test]
    fn no_origin_header_marks_an_api_change_with_the_user_agent() {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("mike-mcp/1.0"));
        let source = change_source_from_headers(&headers);
        assert_eq!(source.via, ChangeVia::Api);
        assert_eq!(source.actor.as_deref(), Some("mike-mcp/1.0"));
    }
    #[test]
    fn encoded_reason_is_preserved_in_the_audit_detail() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-change-reason",
            HeaderValue::from_static("Pause%20%E2%80%94%20repair%20C%2B%2B"),
        );
        assert_eq!(
            change_source_from_headers(&headers).detail["reason"],
            "Pause — repair C++"
        );
    }
}
