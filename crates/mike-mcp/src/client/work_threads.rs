use serde_json::Value;

use super::InspectClient;
use super::http::build_thread_path;
use crate::error::McpError;

#[derive(Debug, Default)]
pub(crate) struct ThreadListQuery<'a> {
    pub(crate) project: &'a str,
    pub(crate) status: Option<&'a str>,
    pub(crate) limit: Option<u64>,
    pub(crate) offset: Option<u64>,
}

#[derive(Debug, Default)]
pub(crate) struct ThreadGetQuery<'a> {
    pub(crate) project: &'a str,
    pub(crate) slug: &'a str,
    pub(crate) history_limit: Option<u64>,
    pub(crate) history_before: Option<i64>,
}

impl InspectClient {
    pub(crate) async fn list_threads(
        &self,
        filter: ThreadListQuery<'_>,
    ) -> Result<Value, McpError> {
        let mut query: Vec<(&str, String)> = vec![("project", filter.project.to_string())];
        if let Some(status) = filter.status {
            query.push(("status", status.to_string()));
        }
        if let Some(limit) = filter.limit {
            query.push(("limit", limit.to_string()));
        }
        if let Some(offset) = filter.offset {
            query.push(("offset", offset.to_string()));
        }
        self.get_json_with_not_found_query("/api/work-threads", &query, "project")
            .await
    }

    pub(crate) async fn get_thread(&self, filter: ThreadGetQuery<'_>) -> Result<Value, McpError> {
        let mut query: Vec<(&str, String)> = vec![("project", filter.project.to_string())];
        if let Some(limit) = filter.history_limit {
            query.push(("history_limit", limit.to_string()));
        }
        if let Some(before) = filter.history_before {
            query.push(("history_before", before.to_string()));
        }
        let path = build_thread_path(filter.slug, &[])?;
        self.get_json_with_not_found_query(&path, &query, &format!("thread `{}`", filter.slug))
            .await
    }

    pub(crate) async fn checkpoint_thread(&self, body: &Value) -> Result<Value, McpError> {
        self.post_json_with_not_found("/api/work-threads", body, "thread")
            .await
    }

    pub(crate) async fn add_thread_entry(
        &self,
        slug: &str,
        body: &Value,
    ) -> Result<Value, McpError> {
        let path = build_thread_path(slug, &["entries"])?;
        self.post_json_with_not_found(&path, body, &format!("thread `{slug}`"))
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
    async fn list_threads_forwards_project_and_status() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/work-threads"))
            .and(query_param("project", "proj-1"))
            .and(query_param("status", "active,review"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"threads": []})),
            )
            .mount(&server)
            .await;
        let value = client
            .list_threads(ThreadListQuery {
                project: "proj-1",
                status: Some("active,review"),
                ..Default::default()
            })
            .await?;
        assert!(value["threads"].is_array());
        Ok(())
    }

    #[tokio::test]
    async fn get_thread_encodes_the_slug_and_forwards_the_history_window() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/work-threads/a%20b"))
            .and(query_param("history_limit", "5"))
            .and(query_param("history_before", "9"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"thread": {}})),
            )
            .mount(&server)
            .await;
        let value = client
            .get_thread(ThreadGetQuery {
                project: "p",
                slug: "a b",
                history_limit: Some(5),
                history_before: Some(9),
            })
            .await?;
        assert!(value["thread"].is_object());
        Ok(())
    }

    #[tokio::test]
    async fn get_thread_404_maps_to_not_found() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/work-threads/missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let err = client
            .get_thread(ThreadGetQuery {
                project: "p",
                slug: "missing",
                ..Default::default()
            })
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("expected NotFound, got Ok"))?;
        assert!(matches!(err, McpError::NotFound(m) if m.contains("missing")));
        Ok(())
    }

    #[tokio::test]
    async fn checkpoint_and_entry_post_their_bodies() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        let checkpoint = serde_json::json!({"project": "p", "slug": "s", "tldr": "t"});
        Mock::given(method("POST"))
            .and(path("/api/work-threads"))
            .and(body_json(&checkpoint))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"created": true})),
            )
            .mount(&server)
            .await;
        let entry = serde_json::json!({"project": "p", "kind": "brief", "body": "b"});
        Mock::given(method("POST"))
            .and(path("/api/work-threads/s/entries"))
            .and(body_json(&entry))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"entry": {}})),
            )
            .mount(&server)
            .await;
        assert_eq!(
            client.checkpoint_thread(&checkpoint).await?["created"],
            true
        );
        assert!(client.add_thread_entry("s", &entry).await?["entry"].is_object());
        Ok(())
    }
}
