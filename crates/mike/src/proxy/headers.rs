use axum::http::{HeaderMap, HeaderValue};

const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

const SENSITIVE_HEADERS: &[&str] = &[
    "x-api-key",
    "authorization",
    "cookie",
    "set-cookie",
    "proxy-authorization",
];

const SENSITIVE_HEADER_MARKERS: &[&str] = &[
    "authorization",
    "cookie",
    "api-key",
    "apikey",
    "auth-token",
    "access-token",
    "session-token",
    "secret",
    "password",
    "credential",
];

pub(crate) fn is_sensitive_header(name: &str) -> bool {
    let lower = name.to_lowercase();
    SENSITIVE_HEADERS.contains(&lower.as_str())
        || lower.ends_with("token")
        || SENSITIVE_HEADER_MARKERS
            .iter()
            .any(|marker| lower.contains(marker))
}

pub(crate) fn captured_header_pairs(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(name, value)| {
            let raw = value.to_str().unwrap_or("");
            let stored = if is_sensitive_header(name.as_str()) {
                mask_header_value(raw)
            } else {
                raw.to_string()
            };
            (name.as_str().to_string(), stored)
        })
        .collect()
}

pub(crate) const MUTATION_EXEMPT_CLIENT_HEADER: &str = "x-mike-client";

pub(crate) const PROMPT_ASSISTANT_CLIENT: &str = "prompt-assistant";

pub(crate) fn is_mutation_exempt_client(headers: &HeaderMap) -> bool {
    headers
        .get(MUTATION_EXEMPT_CLIENT_HEADER)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == PROMPT_ASSISTANT_CLIENT)
}

pub(crate) fn filter_request_headers(headers: &HeaderMap, upstream_host: &str) -> HeaderMap {
    let mut filtered = HeaderMap::new();
    for (name, value) in headers.iter() {
        let name_str = name.as_str().to_lowercase();
        if HOP_BY_HOP.contains(&name_str.as_str())
            || name_str == "host"
            || name_str == "accept-encoding"
            || name_str == "content-length"
        {
            continue;
        }
        filtered.append(name.clone(), value.clone());
    }
    if let Ok(host) = HeaderValue::from_str(upstream_host) {
        filtered.insert("host", host);
    }
    filtered.insert("accept-encoding", HeaderValue::from_static("identity"));
    filtered
}

pub(crate) fn filter_response_headers(headers: &HeaderMap) -> HeaderMap {
    let mut filtered = HeaderMap::new();
    for (name, value) in headers.iter() {
        let name_str = name.as_str().to_lowercase();
        if HOP_BY_HOP.contains(&name_str.as_str()) {
            continue;
        }
        filtered.append(name.clone(), value.clone());
    }
    filtered
}

pub(crate) fn mask_header_value(_value: &str) -> String {
    "[redacted]".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_request_headers_removes_hop_by_hop() {
        let mut headers = HeaderMap::new();
        headers.insert("connection", HeaderValue::from_static("keep-alive"));
        headers.insert("keep-alive", HeaderValue::from_static("timeout=5"));
        headers.insert("x-api-key", HeaderValue::from_static("sk-test"));
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        headers.insert("host", HeaderValue::from_static("127.0.0.1:9988"));

        let filtered = filter_request_headers(&headers, "api.anthropic.com");

        assert!(filtered.get("connection").is_none());
        assert!(filtered.get("keep-alive").is_none());
        assert_eq!(
            filtered.get("x-api-key").map(|v| v.to_str().ok()),
            Some(Some("sk-test"))
        );
        assert_eq!(
            filtered.get("anthropic-version").map(|v| v.to_str().ok()),
            Some(Some("2023-06-01"))
        );
        assert_eq!(
            filtered.get("host").map(|v| v.to_str().ok()),
            Some(Some("api.anthropic.com"))
        );
    }

    #[test]
    fn test_filter_request_headers_sets_host() {
        let headers = HeaderMap::new();
        let filtered = filter_request_headers(&headers, "api.anthropic.com");

        assert_eq!(
            filtered.get("host").map(|v| v.to_str().ok()),
            Some(Some("api.anthropic.com"))
        );
    }

    #[test]
    fn test_filter_response_headers_removes_hop_by_hop() {
        let mut headers = HeaderMap::new();
        headers.insert("transfer-encoding", HeaderValue::from_static("chunked"));
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        headers.insert(
            "anthropic-ratelimit-requests-limit",
            HeaderValue::from_static("100"),
        );

        let filtered = filter_response_headers(&headers);

        assert!(filtered.get("transfer-encoding").is_none());
        assert_eq!(
            filtered.get("content-type").map(|v| v.to_str().ok()),
            Some(Some("application/json"))
        );
        assert_eq!(
            filtered
                .get("anthropic-ratelimit-requests-limit")
                .map(|v| v.to_str().ok()),
            Some(Some("100"))
        );
    }

    #[test]
    fn test_filter_response_headers_preserves_all_non_hop() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", HeaderValue::from_static("30"));
        headers.insert("request-id", HeaderValue::from_static("req-123"));
        headers.insert("x-custom", HeaderValue::from_static("value"));

        let filtered = filter_response_headers(&headers);

        assert_eq!(filtered.len(), 3);
        assert!(filtered.get("retry-after").is_some());
        assert!(filtered.get("request-id").is_some());
        assert!(filtered.get("x-custom").is_some());
    }

    #[test]
    fn test_is_sensitive_header() {
        assert!(is_sensitive_header("x-api-key"));
        assert!(is_sensitive_header("X-Api-Key"));
        assert!(is_sensitive_header("authorization"));
        assert!(is_sensitive_header("Authorization"));
        assert!(is_sensitive_header("cookie"));
        assert!(!is_sensitive_header("content-type"));
        assert!(!is_sensitive_header("anthropic-version"));
        assert!(!is_sensitive_header("x-claude-code-session-id"));
        assert!(!is_sensitive_header("X-Claude-Code-Session-Id"));
        assert!(is_sensitive_header("proxy-authorization"));
    }

    #[test]
    fn credential_shaped_header_names_are_sensitive() {
        for name in [
            "x-goog-api-key",
            "api-key",
            "openai-api-key",
            "x-auth-token",
            "x-access-token",
            "x-amz-security-token",
            "x-csrf-token",
            "x-client-secret",
            "anthropic-auth-token",
            "x-session-token",
        ] {
            assert!(is_sensitive_header(name), "{name} must be masked");
        }
    }

    #[test]
    fn rate_limit_and_usage_headers_are_not_sensitive() {
        for name in [
            "anthropic-ratelimit-tokens-remaining",
            "anthropic-ratelimit-input-tokens-limit",
            "anthropic-ratelimit-output-tokens-reset",
            "anthropic-ratelimit-unified-5h-utilization",
            "anthropic-organization-id",
            "request-id",
            "x-claude-code-agent-id",
        ] {
            assert!(!is_sensitive_header(name), "{name} must stay readable");
        }
    }

    #[test]
    fn captured_response_headers_mask_set_cookie_and_keep_the_rest() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "set-cookie",
            HeaderValue::from_static("__cf_bm=SUPERSECRET; path=/"),
        );
        headers.insert("request-id", HeaderValue::from_static("req_123"));
        headers.insert(
            "anthropic-ratelimit-tokens-remaining",
            HeaderValue::from_static("4000"),
        );

        let stored = captured_header_pairs(&headers);

        let value_of = |name: &str| {
            stored
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(value_of("set-cookie"), Some("[redacted]"));
        assert_eq!(value_of("request-id"), Some("req_123"));
        assert_eq!(
            value_of("anthropic-ratelimit-tokens-remaining"),
            Some("4000")
        );
        assert!(stored.iter().all(|(_, v)| !v.contains("SUPERSECRET")));
    }

    #[test]
    fn test_accept_encoding_rewritten_to_identity() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "accept-encoding",
            HeaderValue::from_static("gzip, deflate, br, zstd"),
        );
        headers.insert("x-api-key", HeaderValue::from_static("sk-test"));

        let filtered = filter_request_headers(&headers, "api.anthropic.com");

        assert_eq!(
            filtered.get("accept-encoding").map(|v| v.to_str().ok()),
            Some(Some("identity"))
        );
        assert_eq!(
            filtered.get("x-api-key").map(|v| v.to_str().ok()),
            Some(Some("sk-test"))
        );
    }

    #[test]
    fn test_accept_encoding_added_when_missing() {
        let headers = HeaderMap::new();
        let filtered = filter_request_headers(&headers, "api.anthropic.com");

        assert_eq!(
            filtered.get("accept-encoding").map(|v| v.to_str().ok()),
            Some(Some("identity"))
        );
    }

    #[test]
    fn test_accept_encoding_replaces_multiple_values() {
        let mut headers = HeaderMap::new();
        headers.append("accept-encoding", HeaderValue::from_static("gzip"));
        headers.append("accept-encoding", HeaderValue::from_static("br"));
        headers.append("accept-encoding", HeaderValue::from_static("zstd"));

        let filtered = filter_request_headers(&headers, "api.anthropic.com");

        assert_eq!(filtered.get_all("accept-encoding").iter().count(), 1);
        assert_eq!(
            filtered.get("accept-encoding").map(|v| v.to_str().ok()),
            Some(Some("identity"))
        );
    }

    #[test]
    fn mask_header_value_is_irreversible() {
        let secret = "sk-ant-api03-SUPERSECRET";
        let masked = mask_header_value(secret);
        assert_eq!(masked, "[redacted]");
        assert!(
            !masked.contains("SUPERSECRET"),
            "no substring of the original may survive masking"
        );
    }

    #[test]
    fn mask_header_value_is_constant_for_short_and_empty_input() {
        assert_eq!(mask_header_value(""), "[redacted]");
        assert_eq!(mask_header_value("ab"), "[redacted]");
    }

    #[test]
    fn is_mutation_exempt_client_true_for_prompt_assistant_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-mike-client",
            HeaderValue::from_static("prompt-assistant"),
        );
        assert!(is_mutation_exempt_client(&headers));
    }

    #[test]
    fn is_mutation_exempt_client_false_without_header() {
        let headers = HeaderMap::new();
        assert!(!is_mutation_exempt_client(&headers));
    }

    #[test]
    fn is_mutation_exempt_client_false_for_unrecognized_value() {
        let mut headers = HeaderMap::new();
        headers.insert("x-mike-client", HeaderValue::from_static("some-other-tool"));
        assert!(!is_mutation_exempt_client(&headers));
    }

    #[test]
    fn is_mutation_exempt_client_false_for_wrong_header_name() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-claude-code-agent-id",
            HeaderValue::from_static("prompt-assistant"),
        );
        assert!(!is_mutation_exempt_client(&headers));
    }
}
