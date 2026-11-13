use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use http_body_util::LengthLimitError;

pub fn is_length_limit_error(err: &axum::Error) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(err);
    while let Some(s) = source {
        if s.is::<LengthLimitError>() {
            return true;
        }
        source = s.source();
    }
    false
}

pub fn payload_too_large(max_bytes: u64) -> Response {
    let body = format!("{{\"error\": \"request body exceeds {max_bytes} bytes\"}}");
    let mut response = (StatusCode::PAYLOAD_TOO_LARGE, body).into_response();
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    response
}

pub fn response_too_large(max_bytes: u64) -> Response {
    let body = format!(
        "{{\"error\": \"upstream response body exceeds {max_bytes} bytes\", \"truncated\": false}}"
    );
    let mut response = (StatusCode::BAD_GATEWAY, body).into_response();
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    response
}

pub fn service_unavailable_capacity(max: usize) -> Response {
    let body = format!("{{\"error\": \"in-flight registry saturated ({max}); retry shortly\"}}");
    let mut response = (StatusCode::SERVICE_UNAVAILABLE, body).into_response();
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    response.headers_mut().insert(
        axum::http::header::RETRY_AFTER,
        axum::http::HeaderValue::from_static("1"),
    );
    response
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use http_body_util::BodyExt;

    #[tokio::test]
    async fn is_length_limit_error_recognises_to_bytes_cap() -> anyhow::Result<()> {
        let body = axum::body::Body::from(vec![0u8; 128]);
        let Err(err) = axum::body::to_bytes(body, 16).await else {
            panic!("to_bytes must reject a body over the cap");
        };
        assert!(
            is_length_limit_error(&err),
            "the cap signal must be found down the source chain"
        );
        Ok(())
    }

    #[tokio::test]
    async fn is_length_limit_error_returns_false_for_unrelated_errors() -> anyhow::Result<()> {
        let err = axum::Error::new(std::io::Error::other("unrelated"));
        assert!(!is_length_limit_error(&err));
        Ok(())
    }

    #[tokio::test]
    async fn payload_too_large_serialises_max_bytes() -> anyhow::Result<()> {
        let response = payload_too_large(4096);
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/json")
        );
        let bytes = response.into_body().collect().await?.to_bytes();
        let parsed: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert_eq!(parsed["error"], "request body exceeds 4096 bytes");
        Ok(())
    }

    #[tokio::test]
    async fn response_too_large_serialises_truncated_false_flag() -> anyhow::Result<()> {
        let response = response_too_large(8192);
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        let bytes = response.into_body().collect().await?.to_bytes();
        let parsed: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert_eq!(parsed["error"], "upstream response body exceeds 8192 bytes");
        assert_eq!(
            parsed["truncated"],
            serde_json::Value::Bool(false),
            "the flag states the body was refused whole, never sliced"
        );
        Ok(())
    }

    #[tokio::test]
    async fn service_unavailable_capacity_carries_retry_after() -> anyhow::Result<()> {
        let response = service_unavailable_capacity(64);
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok()),
            Some("1")
        );
        let bytes = response.into_body().collect().await?.to_bytes();
        let parsed: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert_eq!(
            parsed["error"],
            "in-flight registry saturated (64); retry shortly"
        );
        Ok(())
    }
}
