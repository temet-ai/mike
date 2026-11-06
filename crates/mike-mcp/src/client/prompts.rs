use serde_json::Value;

use super::InspectClient;
use super::http::build_segments_url;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn list_prompt_compositions(&self) -> Result<Value, McpError> {
        self.get_json("/api/prompts/compositions", &[]).await
    }

    pub(crate) async fn get_prompt_composition(&self, id: &str) -> Result<Value, McpError> {
        let path = format!("/api/prompts/compositions/{id}");
        self.get_json_with_not_found(&path, &format!("composition id {id}"))
            .await
    }

    pub(crate) async fn get_prompt_composition_by_sha256(
        &self,
        prompt_sha256: &str,
    ) -> Result<Value, McpError> {
        let url = build_segments_url(
            &self.base_url,
            &["api", "prompts", "compositions", "by-sha256", prompt_sha256],
        )?;
        self.get_json_full_url(url, &format!("composition prompt_sha256 {prompt_sha256}"))
            .await
    }

    pub(crate) async fn create_prompt_composition(&self, body: Value) -> Result<Value, McpError> {
        self.post_json("/api/prompts/compositions", &body).await
    }

    pub(crate) async fn update_prompt_composition(
        &self,
        id: &str,
        body: Value,
    ) -> Result<Value, McpError> {
        let path = format!("/api/prompts/compositions/{id}");
        self.patch_json_with_not_found(&path, &body, &format!("composition id {id}"))
            .await
    }

    pub(crate) async fn delete_prompt_composition(&self, id: &str) -> Result<Value, McpError> {
        let path = format!("/api/prompts/compositions/{id}");
        self.delete_json_with_not_found(&path, &format!("composition id {id}"))
            .await
    }

    pub(crate) async fn list_prompt_anomalies(
        &self,
        query: &[(&str, String)],
    ) -> Result<Value, McpError> {
        self.get_json("/api/prompts/anomalies", query).await
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn start_mock() -> (MockServer, InspectClient) {
        let server = MockServer::start().await;
        let client = InspectClient::new(server.uri());
        (server, client)
    }

    #[tokio::test]
    async fn get_prompt_composition_by_sha256_url_encodes_segment() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/prompts/compositions/by-sha256/abc%2F123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "11111111-1111-1111-1111-111111111111",
                "prompt_sha256": "abc/123",
                "label": "test",
                "version": 1,
                "blocks": [],
                "mode": "observe",
                "notes": null,
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z",
                "applied_count_24h": 0
            })))
            .mount(&server)
            .await;
        let value = client.get_prompt_composition_by_sha256("abc/123").await?;
        assert_eq!(value["prompt_sha256"], "abc/123");
        Ok(())
    }
}
