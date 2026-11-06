use reqwest::StatusCode;
use serde_json::Value;

use super::InspectClient;
use super::http::body_pointer_query;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn list_requests(
        &self,
        offset: Option<u64>,
        limit: Option<u64>,
        order: Option<&str>,
        date_from: Option<&str>,
        date_to: Option<&str>,
    ) -> Result<Value, McpError> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(o) = offset {
            query.push(("offset", o.to_string()));
        }
        if let Some(l) = limit {
            query.push(("limit", l.to_string()));
        }
        if let Some(ord) = order {
            query.push(("order", ord.to_string()));
        }
        if let Some(df) = date_from {
            query.push(("date_from", df.to_string()));
        }
        if let Some(dt) = date_to {
            query.push(("date_to", dt.to_string()));
        }
        self.get_json("/api/requests", &query).await
    }

    pub(crate) async fn get_request(&self, id: u64) -> Result<Value, McpError> {
        let path = format!("/api/requests/{id}");
        self.get_json_with_not_found(&path, &format!("request id {id}"))
            .await
    }

    pub(crate) async fn get_request_summary(&self, id: u64) -> Result<Value, McpError> {
        let path = format!("/api/requests/{id}/summary");
        self.get_json_with_not_found(&path, &format!("request id {id}"))
            .await
    }

    pub(crate) async fn get_request_body(
        &self,
        id: u64,
        pointer: Option<&str>,
    ) -> Result<Value, McpError> {
        let path = format!("/api/requests/{id}/body/request");
        let query = body_pointer_query(pointer);
        self.get_json_with_not_found_query(&path, &query, &format!("request id {id}"))
            .await
    }

    pub(crate) async fn get_request_sent_body(
        &self,
        id: u64,
        pointer: Option<&str>,
    ) -> Result<Value, McpError> {
        let path = format!("/api/requests/{id}/body/request_sent");
        let query = body_pointer_query(pointer);
        self.get_json_with_not_found_query(&path, &query, &format!("request id {id}"))
            .await
    }

    pub(crate) async fn get_request_transformation_manifest(
        &self,
        id: u64,
        pointer: Option<&str>,
    ) -> Result<Value, McpError> {
        let path = format!("/api/requests/{id}/manifest");
        let query = body_pointer_query(pointer);
        self.get_json_with_not_found_query(&path, &query, &format!("request id {id}"))
            .await
    }

    pub(crate) async fn get_response_body(
        &self,
        id: u64,
        pointer: Option<&str>,
    ) -> Result<Value, McpError> {
        let path = format!("/api/requests/{id}/body/response");
        let query = body_pointer_query(pointer);
        let url = format!("{}{path}", self.base_url);
        let response = self
            .client
            .get(&url)
            .query(&query)
            .send()
            .await
            .map_err(|e| {
                McpError::Unreachable(format!(
                    "GET {path} failed: {e} (is the proxy running at {}?)",
                    self.base_url
                ))
            })?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            let body_bytes = response.bytes().await.unwrap_or_default();
            if let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(&body_bytes)
                && parsed.get("status").and_then(|v| v.as_str()) == Some("Pending")
            {
                return Ok(serde_json::json!({
                    "available": false,
                    "status": "Pending"
                }));
            }
            return Err(McpError::NotFound(format!("request id {id}")));
        }
        if status.is_success() {
            self.read_capped_json(response, &path).await
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(McpError::UpstreamStatus {
                status: status.as_u16(),
                body,
            })
        }
    }

    pub(crate) async fn get_accumulated_text(&self, id: u64) -> Result<Value, McpError> {
        let path = format!("/api/requests/{id}/accumulated_text");
        self.get_json_with_not_found(&path, &format!("request id {id}"))
            .await
    }

    pub(crate) async fn get_request_stream_chunks(&self, id: u64) -> Result<Value, McpError> {
        let path = format!("/api/requests/{id}/stream_chunks");
        self.get_json_with_not_found(&path, &format!("request id {id}"))
            .await
    }

    pub(crate) async fn get_request_tool_calls(&self, id: u64) -> Result<Value, McpError> {
        let path = format!("/api/requests/{id}/tool-calls");
        self.get_json_with_not_found(&path, &format!("request id {id}"))
            .await
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn start_mock() -> (MockServer, InspectClient) {
        let server = MockServer::start().await;
        let client = InspectClient::new(server.uri());
        (server, client)
    }

    #[tokio::test]
    async fn list_requests_no_params_returns_json_passthrough() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/requests"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": 1, "path": "/v1/messages"}
            ])))
            .mount(&server)
            .await;

        let value = client.list_requests(None, None, None, None, None).await?;
        assert_eq!(value[0]["id"], 1);
        assert_eq!(value[0]["path"], "/v1/messages");
        Ok(())
    }

    #[tokio::test]
    async fn list_requests_with_query_forwards_offset_and_limit() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/requests"))
            .and(query_param("offset", "10"))
            .and(query_param("limit", "25"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let value = client
            .list_requests(Some(10), Some(25), None, None, None)
            .await?;
        assert!(value.is_array());
        Ok(())
    }

    #[tokio::test]
    async fn list_requests_forwards_order_when_supplied() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/requests"))
            .and(query_param("order", "asc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let value = client
            .list_requests(None, None, Some("asc"), None, None)
            .await?;
        assert!(value.is_array());
        Ok(())
    }

    #[tokio::test]
    async fn get_request_404_maps_to_not_found_error() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/requests/42"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let err = client
            .get_request(42)
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("expected NotFound, got Ok"))?;
        match err {
            McpError::NotFound(msg) => assert!(msg.contains("42")),
            other => anyhow::bail!("expected NotFound, got {other:?}"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn get_request_200_returns_parsed_json() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/requests/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 7,
                "status": "Complete"
            })))
            .mount(&server)
            .await;

        let value = client.get_request(7).await?;
        assert_eq!(value["id"], 7);
        assert_eq!(value["status"], "Complete");
        Ok(())
    }

    #[tokio::test]
    async fn get_request_summary_404_maps_to_not_found() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/requests/99/summary"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let err = client
            .get_request_summary(99)
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("expected NotFound"))?;
        assert!(matches!(err, McpError::NotFound(m) if m.contains("99")));
        Ok(())
    }

    #[tokio::test]
    async fn get_request_body_200_returns_envelope() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/requests/5/body/request"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": "{\"hello\":\"world\"}",
                "byte_size": 17,
                "has_body": true
            })))
            .mount(&server)
            .await;
        let value = client.get_request_body(5, None).await?;
        assert_eq!(value["byte_size"], 17);
        assert_eq!(value["has_body"], true);
        assert!(value["text"].as_str().unwrap_or_default().contains("hello"));
        Ok(())
    }

    #[tokio::test]
    async fn get_request_sent_body_hits_request_sent_path() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/requests/5/body/request_sent"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": "filtered",
                "byte_size": 8,
                "has_body": true
            })))
            .mount(&server)
            .await;
        let value = client.get_request_sent_body(5, None).await?;
        assert_eq!(value["text"], "filtered");
        Ok(())
    }

    #[tokio::test]
    async fn get_request_transformation_manifest_hits_manifest_path() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/requests/5/manifest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "request_id": 5,
                "has_manifest": true,
                "manifest": {
                    "schema_version": 1,
                    "original_sha256": "a",
                    "sent_sha256": "a",
                    "stages": []
                }
            })))
            .mount(&server)
            .await;
        let value = client.get_request_transformation_manifest(5, None).await?;
        assert_eq!(value["has_manifest"], true);
        assert_eq!(value["manifest"]["schema_version"], 1);
        Ok(())
    }

    #[tokio::test]
    async fn get_request_transformation_manifest_404_maps_to_not_found() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/requests/99/manifest"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let err = client
            .get_request_transformation_manifest(99, None)
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("expected NotFound"))?;
        assert!(matches!(err, McpError::NotFound(m) if m.contains("99")));
        Ok(())
    }

    #[tokio::test]
    async fn get_response_body_hits_response_path() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/requests/5/body/response"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": "event: ...",
                "byte_size": 10,
                "has_body": true
            })))
            .mount(&server)
            .await;
        let value = client.get_response_body(5, None).await?;
        assert_eq!(value["byte_size"], 10);
        Ok(())
    }
}
