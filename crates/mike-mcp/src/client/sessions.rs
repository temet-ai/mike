use serde_json::Value;

use super::InspectClient;
use super::{FileWritesQuery, ListSessionToolUsesQuery, SetSessionIdentity, TimeSpentQuery};
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn list_sessions(
        &self,
        order: Option<&str>,
        date_from: Option<&str>,
        date_to: Option<&str>,
    ) -> Result<Value, McpError> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(ord) = order {
            query.push(("order", ord.to_string()));
        }
        if let Some(df) = date_from {
            query.push(("date_from", df.to_string()));
        }
        if let Some(dt) = date_to {
            query.push(("date_to", dt.to_string()));
        }
        self.get_json("/api/sessions", &query).await
    }

    pub(crate) async fn get_session(
        &self,
        id: &str,
        offset: Option<u64>,
        limit: Option<u64>,
        order: Option<&str>,
    ) -> Result<Value, McpError> {
        let path = format!("/api/sessions/{id}");
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
        self.get_json_with_not_found_query(&path, &query, &format!("session id {id}"))
            .await
    }

    pub(crate) async fn list_session_tool_uses(
        &self,
        id: &str,
        filter: ListSessionToolUsesQuery<'_>,
    ) -> Result<Value, McpError> {
        let path = format!("/api/sessions/{id}/tool_uses");
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(o) = filter.offset {
            query.push(("offset", o.to_string()));
        }
        if let Some(l) = filter.limit {
            query.push(("limit", l.to_string()));
        }
        if let Some(ord) = filter.order {
            query.push(("order", ord.to_string()));
        }
        if let Some(inc) = filter.include_results {
            query.push(("include_results", inc.to_string()));
        }
        if let Some(a) = filter.agent {
            query.push(("agent", a.to_string()));
        }
        self.get_json(&path, &query).await
    }

    pub(crate) async fn session_tool_use_stats(
        &self,
        id: &str,
        top_n: Option<u32>,
    ) -> Result<Value, McpError> {
        let path = format!("/api/sessions/{id}/tool_use_stats");
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(n) = top_n {
            query.push(("top_n", n.to_string()));
        }
        self.get_json(&path, &query).await
    }

    pub(crate) async fn get_time_spent(&self, q: &TimeSpentQuery<'_>) -> Result<Value, McpError> {
        let mut query: Vec<(&str, String)> = Vec::new();
        for (key, value) in [
            ("project", q.project),
            ("project_path", q.project_path),
            ("from", q.from),
            ("to", q.to),
            ("session_id", q.session_id),
            ("kind", q.kind),
        ] {
            if let Some(v) = value {
                query.push((key, v.to_string()));
            }
        }
        if let Some(v) = q.days {
            query.push(("days", v.to_string()));
        }
        if let Some(v) = q.include_subagents {
            query.push(("include_subagents", v.to_string()));
        }
        if let Some(v) = q.limit {
            query.push(("limit", v.to_string()));
        }
        if let Some(v) = q.offset {
            query.push(("offset", v.to_string()));
        }
        self.get_json("/api/time_spent", &query).await
    }

    pub(crate) async fn confirm_message_receipt(
        &self,
        session_id: &str,
        message_uuid: &str,
    ) -> Result<Value, McpError> {
        let path = format!("/api/sessions/{session_id}/messages/{message_uuid}/receipt-evidence");
        self.get_json(&path, &[]).await
    }

    pub(crate) async fn list_file_writes(
        &self,
        q: &FileWritesQuery<'_>,
    ) -> Result<Value, McpError> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(v) = q.project {
            query.push(("project", v.to_string()));
        }
        if let Some(v) = q.from {
            query.push(("from", v.to_string()));
        }
        if let Some(v) = q.to {
            query.push(("to", v.to_string()));
        }
        if let Some(v) = q.path {
            query.push(("path", v.to_string()));
        }
        if let Some(v) = q.collisions_only {
            query.push(("collisions_only", v.to_string()));
        }
        if let Some(v) = q.offset {
            query.push(("offset", v.to_string()));
        }
        if let Some(v) = q.limit {
            query.push(("limit", v.to_string()));
        }
        self.get_json("/api/file-writes", &query).await
    }

    pub(crate) async fn list_projects(&self) -> Result<Value, McpError> {
        self.get_json("/api/projects", &[]).await
    }

    pub(crate) async fn get_session_head(&self, session_id: &str) -> Result<Value, McpError> {
        let path = format!("/api/sessions/{session_id}/head");
        self.get_json_with_not_found(&path, &format!("session id {session_id}"))
            .await
    }

    pub(crate) async fn set_session_goal(
        &self,
        req: SetSessionIdentity<'_>,
    ) -> Result<Value, McpError> {
        let path = format!("/api/sessions/{}/goal", req.session_id);
        let mut body = serde_json::Map::new();
        for (key, value) in [
            ("goal", req.goal),
            ("next_steps", req.next_steps),
            ("set_by", req.set_by),
            ("role", req.role),
            ("name", req.name),
            ("instructions", req.instructions),
        ] {
            if let Some(v) = value {
                body.insert(key.to_string(), Value::String(v.to_string()));
            }
        }
        self.put_json_with_not_found(
            &path,
            &Value::Object(body),
            &format!("session id {}", req.session_id),
        )
        .await
    }

    pub(crate) async fn get_session_threads(&self, session_id: &str) -> Result<Value, McpError> {
        let path = format!("/api/sessions/{session_id}/threads");
        self.get_json_with_not_found(&path, "session").await
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
    async fn get_session_forwards_order_with_pagination() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/sessions/sess-ordered"))
            .and(query_param("offset", "5"))
            .and(query_param("limit", "10"))
            .and(query_param("order", "asc"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"requests": []})),
            )
            .mount(&server)
            .await;

        let value = client
            .get_session("sess-ordered", Some(5), Some(10), Some("asc"))
            .await?;
        assert!(value["requests"].is_array());
        Ok(())
    }

    #[tokio::test]
    async fn get_session_404_maps_to_not_found_error() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/sessions/sess-abc"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let err = client
            .get_session("sess-abc", None, None, None)
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("expected NotFound, got Ok"))?;
        match err {
            McpError::NotFound(msg) => assert!(msg.contains("sess-abc")),
            other => anyhow::bail!("expected NotFound, got {other:?}"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn get_session_with_pagination_forwards_offset_and_limit() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/sessions/sess-page"))
            .and(query_param("offset", "20"))
            .and(query_param("limit", "10"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "summary": {"id": "sess-page"},
                "requests": [],
                "total_requests": 42,
                "offset": 20,
                "limit": 10
            })))
            .mount(&server)
            .await;

        let value = client
            .get_session("sess-page", Some(20), Some(10), None)
            .await?;
        assert_eq!(value["total_requests"], 42);
        assert_eq!(value["offset"], 20);
        assert_eq!(value["limit"], 10);
        Ok(())
    }

    #[tokio::test]
    async fn list_sessions_returns_array_passthrough() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/sessions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": "sess-1", "request_count": 3}
            ])))
            .mount(&server)
            .await;
        let value = client.list_sessions(None, None, None).await?;
        assert_eq!(value[0]["id"], "sess-1");
        Ok(())
    }
}
