use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn create_condition(&self, body: &Value) -> Result<Value, McpError> {
        self.post_json("/api/conditions", body).await
    }

    pub(crate) async fn list_conditions(
        &self,
        query: &[(&str, String)],
    ) -> Result<Value, McpError> {
        self.get_json("/api/conditions", query).await
    }

    pub(crate) async fn cancel_condition(&self, condition: &str) -> Result<Value, McpError> {
        self.delete_json_with_not_found(
            &format!("/api/conditions/{condition}"),
            &format!("condition {condition}"),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{body_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn start_mock() -> (MockServer, InspectClient) {
        let server = MockServer::start().await;
        let client = InspectClient::new(server.uri());
        (server, client)
    }

    #[tokio::test]
    async fn create_condition_posts_the_body_verbatim() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        let body = json!({"kind": "time", "after_secs": 60, "brief": "b"});
        Mock::given(method("POST"))
            .and(path("/api/conditions"))
            .and(body_json(&body))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"state": "armed"})))
            .expect(1)
            .mount(&server)
            .await;
        let value = client.create_condition(&body).await?;
        assert_eq!(value["state"], "armed");
        Ok(())
    }

    #[tokio::test]
    async fn list_conditions_forwards_the_filters() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/conditions"))
            .and(query_param("thread", "side-calls"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .expect(1)
            .mount(&server)
            .await;
        let value = client
            .list_conditions(&[("thread", "side-calls".to_string())])
            .await?;
        assert_eq!(value, json!([]));
        Ok(())
    }

    #[tokio::test]
    async fn cancel_condition_deletes_by_uuid() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("DELETE"))
            .and(path("/api/conditions/c1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"state": "cancelled"})))
            .expect(1)
            .mount(&server)
            .await;
        let value = client.cancel_condition("c1").await?;
        assert_eq!(value["state"], "cancelled");
        Ok(())
    }
}
