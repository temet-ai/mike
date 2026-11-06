use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

fn fleet_path(fleet: &str) -> Result<String, McpError> {
    let mut url = reqwest::Url::parse("http://x")
        .map_err(|e| McpError::Parse(format!("bad dummy base: {e}")))?;
    url.path_segments_mut()
        .map_err(|_| McpError::Parse("dummy url cannot be a base".to_string()))?
        .extend(["api", "fleets"])
        .push(fleet);
    Ok(url.path().to_string())
}

impl InspectClient {
    pub(crate) async fn create_fleet(&self, body: &Value) -> Result<Value, McpError> {
        self.post_json("/api/fleets", body).await
    }

    pub(crate) async fn patch_fleet(&self, fleet: &str, body: &Value) -> Result<Value, McpError> {
        self.patch_json_with_not_found(&fleet_path(fleet)?, body, &format!("fleet {fleet}"))
            .await
    }

    pub(crate) async fn get_fleet_view(
        &self,
        fleet: &str,
        query: &[(&str, String)],
    ) -> Result<Value, McpError> {
        self.get_json_with_not_found_query(&fleet_path(fleet)?, query, &format!("fleet {fleet}"))
            .await
    }

    pub(crate) async fn get_fleet_record(
        &self,
        fleet: &str,
        query: &[(&str, String)],
    ) -> Result<Value, McpError> {
        let path = format!("{}/record", fleet_path(fleet)?);
        self.get_json_with_not_found_query(&path, query, &format!("fleet {fleet}"))
            .await
    }

    pub(crate) async fn get_fleet_changes(
        &self,
        fleet: &str,
        query: &[(&str, String)],
    ) -> Result<Value, McpError> {
        let path = format!("{}/changes", fleet_path(fleet)?);
        self.get_json_with_not_found_query(&path, query, &format!("fleet {fleet}"))
            .await
    }

    pub(crate) async fn get_fleet_results(
        &self,
        fleet: &str,
        query: &[(&str, String)],
    ) -> Result<Value, McpError> {
        let path = format!("{}/results", fleet_path(fleet)?);
        self.get_json_with_not_found_query(&path, query, &format!("fleet {fleet}"))
            .await
    }

    pub(crate) async fn commit_fleet(&self, fleet: &str, body: &Value) -> Result<Value, McpError> {
        let path = format!("{}/commit", fleet_path(fleet)?);
        self.post_json(&path, body).await
    }

    pub(crate) async fn integrate_fleet(&self, fleet: &str) -> Result<Value, McpError> {
        let path = format!("{}/integrate", fleet_path(fleet)?);
        self.post_json(&path, &Value::Null).await
    }

    pub(crate) async fn check_fleet(&self, fleet: &str, body: &Value) -> Result<Value, McpError> {
        let path = format!("{}/check", fleet_path(fleet)?);
        self.post_json(&path, body).await
    }

    pub(crate) async fn where_in_fleet(
        &self,
        fleet: &str,
        query: &[(&str, String)],
    ) -> Result<Value, McpError> {
        let path = format!("{}/where", fleet_path(fleet)?);
        self.get_json_with_not_found_query(&path, query, &format!("fleet {fleet}"))
            .await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn start_mock() -> (MockServer, InspectClient) {
        let server = MockServer::start().await;
        let client = InspectClient::new(server.uri());
        (server, client)
    }

    #[test]
    fn fleet_path_escapes_what_would_change_the_route() -> anyhow::Result<()> {
        assert_eq!(fleet_path("night-run")?, "/api/fleets/night-run");
        assert_eq!(fleet_path("a/b")?, "/api/fleets/a%2Fb");
        assert_eq!(fleet_path("a b?c")?, "/api/fleets/a%20b%3Fc");
        Ok(())
    }

    #[tokio::test]
    async fn create_fleet_posts_the_body_verbatim() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        let body =
            json!({"slug": "night-run", "project_path": "/repo", "definition": {"goal": "g"}});
        Mock::given(method("POST"))
            .and(path("/api/fleets"))
            .and(body_json(&body))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"fleet": "f1"})))
            .mount(&server)
            .await;
        let value = client.create_fleet(&body).await?;
        assert_eq!(value["fleet"], "f1");
        Ok(())
    }

    #[tokio::test]
    async fn patch_fleet_sends_the_patch_to_the_fleet_path() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        let body = json!({"patch": {"goal": null}, "ready": true});
        Mock::given(method("PATCH"))
            .and(path("/api/fleets/night-run"))
            .and(body_json(&body))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ready": false})))
            .mount(&server)
            .await;
        let value = client.patch_fleet("night-run", &body).await?;
        assert_eq!(value["ready"], false);
        Ok(())
    }

    #[tokio::test]
    async fn patch_fleet_keeps_the_error_code_of_a_refusal() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("PATCH"))
            .and(path("/api/fleets/f1"))
            .respond_with(ResponseTemplate::new(409).set_body_json(json!({
                "error": "fleet_starting", "message": "the fleet `f1` is starting"
            })))
            .mount(&server)
            .await;
        let refused = client.patch_fleet("f1", &json!({})).await;
        match refused {
            Err(McpError::UpstreamStatus { status, body }) => {
                assert_eq!(status, 409);
                assert!(body.contains("fleet_starting"), "{body}");
            }
            other => panic!("expected UpstreamStatus, got {other:?}"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn patch_fleet_maps_404_to_not_found() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("PATCH"))
            .and(path("/api/fleets/nobody"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let missing = client.patch_fleet("nobody", &json!({})).await;
        assert!(matches!(missing, Err(McpError::NotFound(_))), "{missing:?}");
        Ok(())
    }

    #[tokio::test]
    async fn get_fleet_reads_the_fleet_path_and_maps_404() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/fleets/f1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"state": "draft"})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/fleets/nobody"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        assert_eq!(client.get_fleet_view("f1", &[]).await?["state"], "draft");
        let missing = client.get_fleet_view("nobody", &[]).await;
        assert!(matches!(missing, Err(McpError::NotFound(_))), "{missing:?}");
        Ok(())
    }
}
