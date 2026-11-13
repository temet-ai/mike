use axum::http::{HeaderMap, Method, Uri};
use axum::response::Response;
use bytes::Bytes;

use crate::error::ProxyError;
use crate::state::AppState;

pub async fn send_upstream(
    state: &AppState,
    method: Method,
    uri: &Uri,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<reqwest::Response, ProxyError> {
    let upstream_url =
        super::upstream::build_upstream_url(&state.config.upstream_url, uri.path(), uri.query());
    let upstream_host = super::upstream::extract_host(&state.config.upstream_url);
    let req_headers = crate::extract::headers::filter_request_headers(headers, &upstream_host);
    let upstream_req = state
        .client
        .request(method, &upstream_url)
        .headers(req_headers)
        .body(body)
        .build()
        .map_err(|e| ProxyError::Internal(format!("failed to build upstream request: {e}")))?;
    super::upstream::send_with_retry(&state.client, upstream_req)
        .await
        .map_err(ProxyError::UpstreamConnect)
}

pub async fn forward_without_capture(
    state: &AppState,
    method: Method,
    uri: &Uri,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response, ProxyError> {
    let upstream_resp = send_upstream(state, method, uri, headers, body).await?;
    let status = upstream_resp.status();
    let resp_headers = crate::extract::headers::filter_response_headers(upstream_resp.headers());
    let resp_body = axum::body::Body::from_stream(upstream_resp.bytes_stream());
    let mut builder = Response::builder().status(status);
    for (name, value) in resp_headers.iter() {
        builder = builder.header(name, value);
    }
    builder
        .body(resp_body)
        .map_err(|e| ProxyError::Internal(e.to_string()))
}

#[cfg(test)]
pub mod tests {
    use axum::http::HeaderValue;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn state_pointed_at(upstream_uri: String) -> anyhow::Result<AppState> {
        let mut config = crate::config::ProxyConfig::for_tests_in_memory();
        config.upstream_url = upstream_uri;
        AppState::new(config).await
    }

    #[tokio::test]
    async fn forwards_method_path_query_and_body_to_upstream() -> anyhow::Result<()> {
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(query_param("beta", "true"))
            .and(header("accept-encoding", "identity"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_string(r#"{"ok":true}"#),
            )
            .expect(1)
            .mount(&upstream)
            .await;

        let state = state_pointed_at(upstream.uri()).await?;
        let uri: Uri = "/v1/messages?beta=true".parse()?;
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_static("sk-test"));
        let body = Bytes::from_static(br#"{"model":"m"}"#);

        let response = forward_without_capture(&state, Method::POST, &uri, &headers, body).await?;
        assert_eq!(response.status(), 200);
        Ok(())
    }

    #[tokio::test]
    async fn strips_hop_by_hop_and_forwards_response_headers() -> anyhow::Result<()> {
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("anthropic-ratelimit-requests-limit", "100")
                    .insert_header("connection", "keep-alive")
                    .set_body_string("{}"),
            )
            .expect(1)
            .mount(&upstream)
            .await;

        let state = state_pointed_at(upstream.uri()).await?;
        let uri: Uri = "/v1/messages".parse()?;
        let headers = HeaderMap::new();
        let body = Bytes::from_static(b"{}");

        let response = forward_without_capture(&state, Method::POST, &uri, &headers, body).await?;
        assert_eq!(
            response
                .headers()
                .get("anthropic-ratelimit-requests-limit")
                .and_then(|v| v.to_str().ok()),
            Some("100"),
            "non-hop-by-hop response headers must reach the client"
        );
        assert!(
            response.headers().get("connection").is_none(),
            "hop-by-hop response headers must be stripped"
        );
        Ok(())
    }

    #[tokio::test]
    async fn upstream_error_status_is_forwarded_verbatim() -> anyhow::Result<()> {
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(529).set_body_string(r#"{"error":"overloaded"}"#))
            .expect(1)
            .mount(&upstream)
            .await;

        let state = state_pointed_at(upstream.uri()).await?;
        let uri: Uri = "/v1/messages".parse()?;
        let headers = HeaderMap::new();
        let body = Bytes::from_static(b"{}");

        let response = forward_without_capture(&state, Method::POST, &uri, &headers, body).await?;
        assert_eq!(response.status(), 529);
        Ok(())
    }
}
