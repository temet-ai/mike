use reqwest::Method;

use super::relay::{Route, route};

#[test]
fn relay_route_allows_only_messages_and_read_only_inspect() {
    let cases = [
        (Method::POST, "/v1/messages", Route::Proxy),
        (Method::POST, "/v1/messages/count_tokens", Route::Proxy),
        (Method::GET, "/api/sessions", Route::Inspect),
        (Method::GET, "/api/requests/12/body", Route::Inspect),
    ];
    for (method, path, expected) in cases {
        assert_eq!(route(&method, path), expected, "{method} {path}");
    }
    let refused = [
        (Method::POST, "/api/sessions"),
        (Method::PATCH, "/api/wright/engineer-limits"),
        (Method::DELETE, "/api/lessons/3"),
        (Method::PUT, "/api/settings"),
        (Method::GET, "/api/wright/problems"),
        (Method::GET, "/api/wright"),
        (Method::GET, "/API/WRIGHT/findings"),
        (Method::GET, "/api/settings/assistant/anthropic/credentials"),
        (Method::GET, "/api/config/snapshot"),
        (Method::GET, "/api/ws"),
        (Method::GET, "/api/%77right/problems"),
        (Method::GET, "/api/sessions/../wright/problems"),
        (Method::GET, "//api/wright"),
        (Method::GET, "/v1/messages"),
        (Method::POST, "/v1/messages/batches"),
        (Method::POST, "/v1/complete"),
        (Method::GET, "/metrics"),
        (Method::GET, "/"),
    ];
    for (method, path) in refused {
        assert!(
            matches!(route(&method, path), Route::Refuse(_)),
            "{method} {path} must be refused"
        );
    }
}
