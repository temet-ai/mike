use std::borrow::Cow;

use rmcp::model::{ErrorCode, ErrorData as RmcpError};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub(crate) enum McpError {
    #[error("proxy inspect bind unreachable: {0}")]
    Unreachable(String),

    #[error("resource not found: {0}")]
    NotFound(String),

    #[error("inspect API returned {status}: {body}")]
    UpstreamStatus { status: u16, body: String },

    #[error("response parse failed: {0}")]
    Parse(String),

    #[error(
        "inspect API response for {endpoint_path} exceeded {limit_bytes}-byte cap (actual: {actual_bytes:?})"
    )]
    ResponseTooLarge {
        actual_bytes: Option<u64>,
        endpoint_path: String,
        limit_bytes: u64,
    },
}

impl McpError {
    pub(crate) fn into_rmcp(self) -> RmcpError {
        match self {
            McpError::NotFound(_) => {
                let msg: Cow<'static, str> = self.to_string().into();
                RmcpError::resource_not_found(msg, None)
            }
            McpError::ResponseTooLarge {
                ref actual_bytes,
                ref endpoint_path,
                limit_bytes,
            } => {
                let data = json!({
                    "kind": "response_too_large",
                    "endpoint_path": endpoint_path,
                    "limit_bytes": limit_bytes,
                    "actual_bytes": actual_bytes,
                });
                let msg: Cow<'static, str> = self.to_string().into();
                RmcpError::new(ErrorCode::INTERNAL_ERROR, msg, Some(data))
            }
            _ => {
                let msg: Cow<'static, str> = self.to_string().into();
                RmcpError::new(ErrorCode::INTERNAL_ERROR, msg, None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_unreachable_includes_reason() {
        let err = McpError::Unreachable("connection refused".to_string());
        assert_eq!(
            err.to_string(),
            "proxy inspect bind unreachable: connection refused"
        );
    }

    #[test]
    fn display_not_found_includes_resource() {
        let err = McpError::NotFound("request id 42".to_string());
        assert_eq!(err.to_string(), "resource not found: request id 42");
    }

    #[test]
    fn display_upstream_status_includes_status_and_body() {
        let err = McpError::UpstreamStatus {
            status: 500,
            body: "server exploded".to_string(),
        };
        assert_eq!(err.to_string(), "inspect API returned 500: server exploded");
    }

    #[test]
    fn display_response_too_large_includes_path_and_limit() {
        let err = McpError::ResponseTooLarge {
            actual_bytes: Some(1_073_741_824),
            endpoint_path: "/api/requests".to_string(),
            limit_bytes: 268_435_456,
        };
        let msg = err.to_string();
        assert!(msg.contains("/api/requests"));
        assert!(msg.contains("268435456"));
        assert!(msg.contains("1073741824"));
    }

    #[test]
    fn into_rmcp_not_found_maps_to_resource_not_found_code() {
        let err = McpError::NotFound("id 7".to_string());
        let rmcp_err = err.into_rmcp();
        assert_eq!(rmcp_err.code, ErrorCode::RESOURCE_NOT_FOUND);
        assert_eq!(rmcp_err.message, "resource not found: id 7");
    }

    #[test]
    fn into_rmcp_unreachable_maps_to_internal_error_code() {
        let err = McpError::Unreachable("econnrefused".to_string());
        let rmcp_err = err.into_rmcp();
        assert_eq!(rmcp_err.code, ErrorCode::INTERNAL_ERROR);
        assert!(rmcp_err.message.contains("econnrefused"));
    }

    #[test]
    fn into_rmcp_parse_error_maps_to_internal_error_code() {
        let err = McpError::Parse("bad json".to_string());
        let rmcp_err = err.into_rmcp();
        assert_eq!(rmcp_err.code, ErrorCode::INTERNAL_ERROR);
    }

    #[test]
    fn into_rmcp_response_too_large_maps_to_internal_error_with_structured_data()
    -> anyhow::Result<()> {
        let err = McpError::ResponseTooLarge {
            actual_bytes: Some(500_000_000),
            endpoint_path: "/api/requests/42/body/response".to_string(),
            limit_bytes: 268_435_456,
        };
        let rmcp_err = err.into_rmcp();
        assert_eq!(rmcp_err.code, ErrorCode::INTERNAL_ERROR);
        let data = rmcp_err
            .data
            .ok_or_else(|| anyhow::anyhow!("expected structured data on ResponseTooLarge"))?;
        assert_eq!(data["kind"], "response_too_large");
        assert_eq!(data["endpoint_path"], "/api/requests/42/body/response");
        assert_eq!(data["limit_bytes"], 268_435_456);
        assert_eq!(data["actual_bytes"], 500_000_000);
        Ok(())
    }

    #[test]
    fn into_rmcp_response_too_large_with_unknown_actual_bytes_serialises_null() -> anyhow::Result<()>
    {
        let err = McpError::ResponseTooLarge {
            actual_bytes: None,
            endpoint_path: "/api/metrics".to_string(),
            limit_bytes: 268_435_456,
        };
        let rmcp_err = err.into_rmcp();
        let data = rmcp_err
            .data
            .ok_or_else(|| anyhow::anyhow!("expected structured data"))?;
        assert!(data["actual_bytes"].is_null());
        Ok(())
    }
}
