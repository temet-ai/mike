use serde_json::Value;

use super::InspectClient;
use super::RegisterWorkspaceSession;
use super::http::build_workspace_roster_path;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn get_workspace_roster(
        &self,
        project_path: &str,
        include_inactive: bool,
    ) -> Result<Value, McpError> {
        let path = build_workspace_roster_path(project_path)?;
        let query: &[(&str, String)] = if include_inactive {
            &[("include_inactive", "true".to_string())]
        } else {
            &[]
        };
        self.get_json_with_not_found_query(&path, query, &format!("project {project_path}"))
            .await
    }

    pub(crate) async fn workspace_heartbeat(&self, session_id: &str) -> Result<Value, McpError> {
        let body = serde_json::json!({ "session_id": session_id });
        self.post_json_with_not_found(
            "/api/workspace/heartbeat",
            &body,
            &format!("workspace registration for session {session_id}"),
        )
        .await
    }

    pub(crate) async fn register_workspace_session(
        &self,
        req: RegisterWorkspaceSession<'_>,
    ) -> Result<Value, McpError> {
        let mut body = serde_json::Map::new();
        body.insert(
            "session_id".to_string(),
            Value::String(req.session_id.to_string()),
        );
        body.insert("cwd".to_string(), Value::String(req.cwd.to_string()));
        for (key, value) in [
            ("repo_root", req.repo_root),
            ("location_uuid", req.location_uuid),
            ("registered_via", req.registered_via),
            ("goal", req.goal),
            ("next_steps", req.next_steps),
            ("role", req.role),
            ("name", req.name),
            ("instructions", req.instructions),
        ] {
            if let Some(v) = value {
                body.insert(key.to_string(), Value::String(v.to_string()));
            }
        }
        if let Some(v) = req.pid {
            body.insert("pid".to_string(), Value::Number(v.into()));
        }
        self.post_json("/api/workspace/register", &Value::Object(body))
            .await
    }

    pub(crate) async fn end_workspace_registration(
        &self,
        session_id: &str,
        pid: Option<i32>,
    ) -> Result<Value, McpError> {
        let path = match pid {
            Some(pid) => format!("/api/workspace/register/{session_id}/end?pid={pid}"),
            None => format!("/api/workspace/register/{session_id}/end"),
        };
        self.post_empty_with_not_found(
            &path,
            &format!("workspace registration for session {session_id}"),
        )
        .await
    }

    pub(crate) async fn get_fleet_graph(
        &self,
        project: &str,
        window_hours: Option<i32>,
    ) -> Result<Value, McpError> {
        let mut query: Vec<(&str, String)> = vec![("project", project.to_string())];
        if let Some(h) = window_hours {
            query.push(("window_hours", h.to_string()));
        }
        self.get_json_with_not_found_query(
            "/api/fleet/graph",
            &query,
            &format!("project {project}"),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn start_mock() -> (MockServer, InspectClient) {
        let server = MockServer::start().await;
        let client = InspectClient::new(server.uri());
        (server, client)
    }

    #[tokio::test]
    async fn get_workspace_roster_default_sends_no_include_inactive_param() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/workspace/projects/proj/roster"))
            .and(query_param_is_missing("include_inactive"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "project_id": 1, "project_path": "proj", "roster": []
            })))
            .mount(&server)
            .await;

        let value = client.get_workspace_roster("proj", false).await?;
        assert_eq!(value["roster"], serde_json::json!([]));
        Ok(())
    }

    #[tokio::test]
    async fn get_workspace_roster_include_inactive_forwards_query_param() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/workspace/projects/proj/roster"))
            .and(query_param("include_inactive", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "project_id": 1, "project_path": "proj", "roster": [{"session_id": "x"}]
            })))
            .mount(&server)
            .await;

        let value = client.get_workspace_roster("proj", true).await?;
        assert_eq!(value["roster"][0]["session_id"], "x");
        Ok(())
    }
}
