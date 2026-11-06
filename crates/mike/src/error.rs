use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ProxyError {
    #[error("upstream connection failed: {0}")]
    UpstreamConnect(#[source] reqwest::Error),

    #[error("request body read failed: {0}")]
    BodyRead(String),

    #[error("internal: {0}")]
    Internal(String),
}

impl ProxyError {
    fn chain(&self) -> String {
        let mut parts = vec![self.to_string()];
        let mut src = std::error::Error::source(self);
        while let Some(e) = src {
            parts.push(e.to_string());
            src = e.source();
        }
        parts.join(" -> ")
    }

    fn status_and_message(&self) -> (StatusCode, &'static str) {
        match self {
            ProxyError::UpstreamConnect(_) => (StatusCode::BAD_GATEWAY, "upstream unreachable"),
            ProxyError::BodyRead(_) => (StatusCode::BAD_REQUEST, "body read failed"),
            ProxyError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal error"),
        }
    }

    fn log(&self) {
        let ProxyError::UpstreamConnect(e) = self else {
            tracing::error!(error = %self.chain(), "proxy error");
            return;
        };
        tracing::error!(
            error = %self.chain(),
            is_timeout = e.is_timeout(),
            is_connect = e.is_connect(),
            is_request = e.is_request(),
            is_body = e.is_body(),
            "proxy error: upstream request failed",
        );
    }
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let (status, msg) = self.status_and_message();
        self.log();
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn body_text(resp: Response) -> anyhow::Result<(StatusCode, String)> {
        let (parts, body) = resp.into_parts();
        let bytes = axum::body::to_bytes(body, usize::MAX).await?;
        let text = String::from_utf8(bytes.to_vec())?;
        Ok((parts.status, text))
    }

    #[test]
    fn display_body_read_includes_inner_message() {
        let err = ProxyError::BodyRead("stream ended prematurely".to_string());
        assert_eq!(
            err.to_string(),
            "request body read failed: stream ended prematurely"
        );
    }

    #[test]
    fn display_internal_includes_inner_message() {
        let err = ProxyError::Internal("oops".to_string());
        assert_eq!(err.to_string(), "internal: oops");
    }

    #[tokio::test]
    async fn into_response_body_read_returns_400() -> anyhow::Result<()> {
        let err = ProxyError::BodyRead("short read".to_string());
        let (status, body) = body_text(err.into_response()).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body, r#"{"error":"body read failed"}"#);
        Ok(())
    }

    #[tokio::test]
    async fn into_response_internal_returns_500() -> anyhow::Result<()> {
        let err = ProxyError::Internal("db down".to_string());
        let (status, body) = body_text(err.into_response()).await?;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, r#"{"error":"internal error"}"#);
        Ok(())
    }

    #[tokio::test]
    async fn into_response_upstream_connect_returns_502() -> anyhow::Result<()> {
        let reqwest_err = match reqwest::Client::new()
            .get("http://127.0.0.1:1")
            .timeout(std::time::Duration::from_millis(100))
            .send()
            .await
        {
            Ok(_) => anyhow::bail!("unexpected success connecting to 127.0.0.1:1"),
            Err(e) => e,
        };

        let err = ProxyError::UpstreamConnect(reqwest_err);
        let (status, body) = body_text(err.into_response()).await?;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(body, r#"{"error":"upstream unreachable"}"#);
        Ok(())
    }
}
