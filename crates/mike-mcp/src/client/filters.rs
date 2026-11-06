use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn list_filter_rules(&self) -> Result<Value, McpError> {
        self.get_json("/api/filter/rules", &[]).await
    }

    pub(crate) async fn create_filter_rule(&self, body: Value) -> Result<Value, McpError> {
        self.post_json("/api/filter/rules", &body).await
    }

    pub(crate) async fn update_filter_rule(
        &self,
        id: &str,
        body: Value,
    ) -> Result<Value, McpError> {
        let path = format!("/api/filter/rules/{id}");
        self.patch_json_with_not_found(&path, &body, &format!("filter rule id {id}"))
            .await
    }

    pub(crate) async fn delete_filter_rule(&self, id: &str) -> Result<Value, McpError> {
        let path = format!("/api/filter/rules/{id}");
        self.delete_json_with_not_found(&path, &format!("filter rule id {id}"))
            .await
    }

    pub(crate) async fn dry_run_filter_rules(&self, body: Value) -> Result<Value, McpError> {
        self.post_json("/api/filter/rules/dry-run", &body).await
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
    async fn create_filter_rule_posts_body_and_returns_envelope() -> anyhow::Result<()> {
        use wiremock::matchers::body_json;
        let (server, client) = start_mock().await;
        Mock::given(method("POST"))
            .and(path("/api/filter/rules"))
            .and(body_json(serde_json::json!({
                "kind": "text_substring",
                "value": "weasel",
                "action": "drop"
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "rule": {
                    "id": "abc",
                    "kind": "text_substring",
                    "value": "weasel",
                    "action": "drop",
                    "enabled": true,
                    "source": "api",
                    "updated_at": "2026-01-01T00:00:00Z"
                },
                "warnings": []
            })))
            .mount(&server)
            .await;
        let value = client
            .create_filter_rule(serde_json::json!({
                "kind": "text_substring",
                "value": "weasel",
                "action": "drop"
            }))
            .await?;
        assert_eq!(value["rule"]["id"], "abc");
        assert!(value["warnings"].is_array());
        Ok(())
    }

    #[tokio::test]
    async fn update_filter_rule_404_maps_to_not_found() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("PATCH"))
            .and(path("/api/filter/rules/missing"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "error": "no filter rule with that id"
            })))
            .mount(&server)
            .await;
        let err = client
            .update_filter_rule("missing", serde_json::json!({ "enabled": false }))
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("expected NotFound, got Ok"))?;
        assert!(matches!(err, McpError::NotFound(m) if m.contains("missing")));
        Ok(())
    }

    #[tokio::test]
    async fn delete_filter_rule_204_normalises_to_deleted_marker() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("DELETE"))
            .and(path("/api/filter/rules/abc"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let value = client.delete_filter_rule("abc").await?;
        assert_eq!(value["deleted"], true);
        assert!(value["warnings"].is_array());
        Ok(())
    }

    #[tokio::test]
    async fn delete_filter_rule_200_with_warnings_passes_through() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("DELETE"))
            .and(path("/api/filter/rules/abc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "warnings": ["example_warning"]
            })))
            .mount(&server)
            .await;
        let value = client.delete_filter_rule("abc").await?;
        assert_eq!(value["warnings"][0], "example_warning");
        assert_eq!(value["deleted"], true);
        Ok(())
    }

    #[tokio::test]
    async fn list_filter_rules_returns_array_passthrough() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/filter/rules"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "r1", "kind": "text_substring", "value": "x", "action": "drop"}
            ])))
            .mount(&server)
            .await;
        let value = client.list_filter_rules().await?;
        assert_eq!(value[0]["id"], "r1");
        Ok(())
    }
}
