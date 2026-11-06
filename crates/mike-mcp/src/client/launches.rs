use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn spawn_session(&self, body: &Value) -> Result<Value, McpError> {
        self.post_json("/api/launches", body).await
    }

    pub(crate) async fn list_launches(&self, run: Option<&str>) -> Result<Value, McpError> {
        let query: &[(&str, String)] = if let Some(run) = run {
            &[("run", run.to_string())]
        } else {
            &[]
        };
        self.get_json("/api/launches", query).await
    }

    pub(crate) async fn report_tools_listed(&self, session: &str) -> Result<(), McpError> {
        self.post_empty_with_not_found(
            &format!("/api/launches/{session}/mcp"),
            &format!("session {session}"),
        )
        .await
        .map(|_| ())
    }

    pub(crate) async fn kill_session(&self, session: &str) -> Result<Value, McpError> {
        self.delete_json_with_not_found(
            &format!("/api/launches/{session}"),
            &format!("launch for session {session}"),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn start_mock() -> (MockServer, InspectClient) {
        let server = MockServer::start().await;
        let client = InspectClient::new(server.uri());
        (server, client)
    }

    #[tokio::test]
    async fn spawn_session_posts_the_body_verbatim() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        let body = serde_json::json!({"folder": "/repo"});
        Mock::given(method("POST"))
            .and(path("/api/launches"))
            .and(body_json(&body))
            .respond_with(ResponseTemplate::new(202).set_body_json(serde_json::json!({
                "launch_id": 1, "session": "00000000-0000-0000-0000-000000000000", "backend": "herdr"
            })))
            .mount(&server)
            .await;
        let value = client.spawn_session(&body).await?;
        assert_eq!(value["launch_id"], 1);
        Ok(())
    }

    #[tokio::test]
    async fn report_tools_listed_posts_to_the_session_route() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("POST"))
            .and(path("/api/launches/abc/mcp"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        client.report_tools_listed("abc").await?;
        Ok(())
    }

    #[tokio::test]
    async fn list_launches_without_run_sends_no_query() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/launches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        let value = client.list_launches(None).await?;
        assert_eq!(value, serde_json::json!([]));
        Ok(())
    }

    #[tokio::test]
    async fn list_launches_with_run_forwards_the_query_param() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/launches"))
            .and(query_param("run", "r1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"run_id": "r1"}
            ])))
            .mount(&server)
            .await;
        let value = client.list_launches(Some("r1")).await?;
        assert_eq!(value[0]["run_id"], "r1");
        Ok(())
    }

    #[tokio::test]
    async fn kill_session_deletes_by_session_path() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("DELETE"))
            .and(path("/api/launches/s1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "session": "s1", "closed": true
            })))
            .mount(&server)
            .await;
        let value = client.kill_session("s1").await?;
        assert_eq!(value["closed"], true);
        Ok(())
    }
}
