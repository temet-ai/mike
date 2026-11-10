use serde_json::Value;

use super::CallerScope;
use super::InspectClient;
use super::RecordLessonRequest;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn recall_project_context(
        &self,
        project_path: &str,
        query: Option<&str>,
        scope: CallerScope<'_>,
    ) -> Result<Value, McpError> {
        let mut q = vec![("project", project_path.to_string())];
        if let Some(query) = query {
            q.push(("query", query.to_string()));
        }
        if let Some(session_id) = scope.session_id {
            q.push(("session_id", session_id.to_string()));
        }
        if let Some(include) = scope.include_own_turn {
            q.push(("include_own_turn", include.to_string()));
        }
        self.get_json("/api/recall/project-context", &q).await
    }

    pub(crate) async fn recall_similar_turns(
        &self,
        project_path: &str,
        query: &str,
        limit: Option<u64>,
        scope: CallerScope<'_>,
    ) -> Result<Value, McpError> {
        let mut body = serde_json::Map::new();
        body.insert(
            "project".to_string(),
            Value::String(project_path.to_string()),
        );
        body.insert("query".to_string(), Value::String(query.to_string()));
        if let Some(l) = limit {
            body.insert("limit".to_string(), Value::Number(l.into()));
        }
        if let Some(session_id) = scope.session_id {
            body.insert(
                "session_id".to_string(),
                Value::String(session_id.to_string()),
            );
        }
        if let Some(include) = scope.include_own_turn {
            body.insert("include_own_turn".to_string(), Value::Bool(include));
        }
        self.post_json("/api/recall/similar-turns", &Value::Object(body))
            .await
    }

    pub(crate) async fn record_lesson(
        &self,
        project_path: &str,
        kind: &str,
        text: &str,
        extra: RecordLessonRequest<'_>,
    ) -> Result<Value, McpError> {
        let mut body = serde_json::Map::new();
        body.insert(
            "project_path".to_string(),
            Value::String(project_path.to_string()),
        );
        body.insert("kind".to_string(), Value::String(kind.to_string()));
        body.insert("text".to_string(), Value::String(text.to_string()));
        if let Some(topic) = extra.topic {
            body.insert("topic".to_string(), Value::String(topic.to_owned()));
        }
        if let Some(t) = extra.source_turn_id {
            body.insert("source_turn_id".to_string(), Value::Number(t.into()));
        }
        if let Some(name) = extra.agent_name {
            body.insert("agent_name".to_string(), Value::String(name.to_string()));
        }
        if let Some(sid) = extra.session_id {
            body.insert("session_id".to_string(), Value::String(sid.to_string()));
        }
        if let Some(id) = extra.supersedes {
            body.insert("supersedes".to_string(), Value::Number(id.into()));
        }
        self.post_json("/api/lessons", &Value::Object(body)).await
    }

    pub(crate) async fn list_lessons(
        &self,
        project_path: &str,
        order: Option<&str>,
    ) -> Result<Value, McpError> {
        let mut q = vec![("project", project_path.to_string())];
        if let Some(ord) = order {
            q.push(("order", ord.to_string()));
        }
        self.get_json("/api/lessons", &q).await
    }

    pub(crate) async fn delete_lesson(&self, id: &str) -> Result<Value, McpError> {
        let path = format!("/api/lessons/{id}");
        self.delete_json_with_not_found(&path, &format!("lesson id {id}"))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn record_lesson_omits_absent_topic_for_existing_clients() -> anyhow::Result<()> {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/api/lessons"))
            .and(body_json(json!({"project_path":"project","kind":"convention","text":"Reuse checked results"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id":1})))
            .expect(1).mount(&server).await;
        let client = InspectClient::new(server.uri());
        assert_eq!(
            client
                .record_lesson(
                    "project",
                    "convention",
                    "Reuse checked results",
                    RecordLessonRequest::default()
                )
                .await?["id"],
            1
        );
        Ok(())
    }

    #[tokio::test]
    async fn record_lesson_surfaces_server_rejection() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/lessons"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({"error":"invalid scope"})),
            )
            .mount(&server)
            .await;
        let client = InspectClient::new(server.uri());
        assert!(
            client
                .record_lesson(
                    "project",
                    "convention",
                    "rule",
                    RecordLessonRequest {
                        topic: Some("shared:checks"),
                        ..Default::default()
                    }
                )
                .await
                .is_err()
        );
    }
}
