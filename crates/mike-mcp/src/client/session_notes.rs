use serde_json::Value;

use super::InspectClient;
use super::http::build_workspace_notes_path;
use crate::error::McpError;

pub(crate) struct WriteSessionNote<'a> {
    pub(crate) session_id: &'a str,
    pub(crate) note_uuid: Option<&'a str>,
    pub(crate) kind: Option<&'a str>,
    pub(crate) title: Option<&'a str>,
    pub(crate) body: Option<&'a str>,
    pub(crate) status: Option<&'a str>,
    pub(crate) tags: Option<&'a [String]>,
    pub(crate) data: Option<&'a Value>,
}

#[derive(Debug, Default)]
pub(crate) struct SessionNotesQuery<'a> {
    pub(crate) kind: Option<&'a str>,
    pub(crate) status: Option<&'a str>,
    pub(crate) tag: Option<&'a str>,
    pub(crate) q: Option<&'a str>,
    pub(crate) limit: Option<u64>,
    pub(crate) offset: Option<u64>,
    pub(crate) order: Option<&'a str>,
}

#[derive(Debug, Default)]
pub(crate) struct ProjectNotesQuery<'a> {
    pub(crate) role: Option<&'a str>,
    pub(crate) name: Option<&'a str>,
    pub(crate) kind: Option<&'a str>,
    pub(crate) status: Option<&'a str>,
    pub(crate) tag: Option<&'a str>,
    pub(crate) q: Option<&'a str>,
    pub(crate) session_uuid: Option<&'a str>,
    pub(crate) limit: Option<u64>,
    pub(crate) offset: Option<u64>,
    pub(crate) order: Option<&'a str>,
}

fn note_body(req: &WriteSessionNote<'_>) -> Value {
    let mut body = serde_json::Map::new();
    for (key, value) in [
        ("kind", req.kind),
        ("title", req.title),
        ("body", req.body),
        ("status", req.status),
    ] {
        if let Some(v) = value {
            body.insert(key.to_string(), Value::String(v.to_string()));
        }
    }
    if let Some(tags) = req.tags {
        body.insert(
            "tags".to_string(),
            Value::Array(tags.iter().cloned().map(Value::String).collect()),
        );
    }
    if let Some(data) = req.data {
        body.insert("data".to_string(), data.clone());
    }
    Value::Object(body)
}

fn push_notes_query<'a>(query: &mut Vec<(&'a str, String)>, filter: &SessionNotesQuery<'a>) {
    if let Some(v) = filter.kind {
        query.push(("kind", v.to_string()));
    }
    if let Some(v) = filter.status {
        query.push(("status", v.to_string()));
    }
    if let Some(v) = filter.tag {
        query.push(("tag", v.to_string()));
    }
    if let Some(v) = filter.q {
        query.push(("q", v.to_string()));
    }
    if let Some(v) = filter.limit {
        query.push(("limit", v.to_string()));
    }
    if let Some(v) = filter.offset {
        query.push(("offset", v.to_string()));
    }
    if let Some(v) = filter.order {
        query.push(("order", v.to_string()));
    }
}

fn push_project_notes_query<'a>(
    query: &mut Vec<(&'a str, String)>,
    filter: &ProjectNotesQuery<'a>,
) {
    if let Some(v) = filter.role {
        query.push(("role", v.to_string()));
    }
    if let Some(v) = filter.name {
        query.push(("name", v.to_string()));
    }
    if let Some(v) = filter.session_uuid {
        query.push(("session_uuid", v.to_string()));
    }
    push_notes_query(
        query,
        &SessionNotesQuery {
            kind: filter.kind,
            status: filter.status,
            tag: filter.tag,
            q: filter.q,
            limit: filter.limit,
            offset: filter.offset,
            order: filter.order,
        },
    );
}

impl InspectClient {
    pub(crate) async fn write_session_note(
        &self,
        req: WriteSessionNote<'_>,
    ) -> Result<Value, McpError> {
        let body = note_body(&req);
        match req.note_uuid {
            Some(note_uuid) => {
                let path = format!("/api/sessions/{}/notes/{note_uuid}", req.session_id);
                self.patch_json_with_not_found(&path, &body, &format!("session note {note_uuid}"))
                    .await
            }
            None => {
                let path = format!("/api/sessions/{}/notes", req.session_id);
                self.post_json_with_not_found(
                    &path,
                    &body,
                    &format!("session id {}", req.session_id),
                )
                .await
            }
        }
    }

    pub(crate) async fn list_session_notes(
        &self,
        session_id: &str,
        filter: SessionNotesQuery<'_>,
    ) -> Result<Value, McpError> {
        let path = format!("/api/sessions/{session_id}/notes");
        let mut query: Vec<(&str, String)> = Vec::new();
        push_notes_query(&mut query, &filter);
        self.get_json_with_not_found_query(&path, &query, &format!("session id {session_id}"))
            .await
    }

    pub(crate) async fn list_project_notes(
        &self,
        project_path: &str,
        filter: ProjectNotesQuery<'_>,
    ) -> Result<Value, McpError> {
        let path = build_workspace_notes_path(project_path)?;
        let mut query: Vec<(&str, String)> = Vec::new();
        push_project_notes_query(&mut query, &filter);
        self.get_json_with_not_found_query(&path, &query, &format!("project {project_path}"))
            .await
    }

    pub(crate) async fn delete_session_note(
        &self,
        session_id: &str,
        note_uuid: &str,
    ) -> Result<Value, McpError> {
        let path = format!("/api/sessions/{session_id}/notes/{note_uuid}");
        self.delete_json_with_not_found(&path, &format!("session note {note_uuid}"))
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
    async fn write_session_note_without_note_uuid_posts_new_note() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("POST"))
            .and(path("/api/sessions/sess-1/notes"))
            .and(body_json(serde_json::json!({
                "kind": "finding",
                "title": "found it"
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "note_uuid": "n1", "title": "found it"
            })))
            .mount(&server)
            .await;
        let value = client
            .write_session_note(WriteSessionNote {
                session_id: "sess-1",
                note_uuid: None,
                kind: Some("finding"),
                title: Some("found it"),
                body: None,
                status: None,
                tags: None,
                data: None,
            })
            .await?;
        assert_eq!(value["note_uuid"], "n1");
        Ok(())
    }

    #[tokio::test]
    async fn write_session_note_with_note_uuid_patches_existing_note() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("PATCH"))
            .and(path("/api/sessions/sess-1/notes/n1"))
            .and(body_json(serde_json::json!({"status": "done"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "note_uuid": "n1", "status": "done"
            })))
            .mount(&server)
            .await;
        let value = client
            .write_session_note(WriteSessionNote {
                session_id: "sess-1",
                note_uuid: Some("n1"),
                kind: None,
                title: None,
                body: None,
                status: Some("done"),
                tags: None,
                data: None,
            })
            .await?;
        assert_eq!(value["status"], "done");
        Ok(())
    }

    #[tokio::test]
    async fn write_session_note_patch_404_maps_to_not_found() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("PATCH"))
            .and(path("/api/sessions/sess-1/notes/missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let err = client
            .write_session_note(WriteSessionNote {
                session_id: "sess-1",
                note_uuid: Some("missing"),
                kind: None,
                title: None,
                body: None,
                status: None,
                tags: None,
                data: None,
            })
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("expected NotFound, got Ok"))?;
        assert!(matches!(err, McpError::NotFound(m) if m.contains("missing")));
        Ok(())
    }

    #[tokio::test]
    async fn list_session_notes_forwards_filters() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/sessions/sess-1/notes"))
            .and(query_param("kind", "finding"))
            .and(query_param("limit", "10"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"notes": []})),
            )
            .mount(&server)
            .await;
        let value = client
            .list_session_notes(
                "sess-1",
                SessionNotesQuery {
                    kind: Some("finding"),
                    limit: Some(10),
                    ..Default::default()
                },
            )
            .await?;
        assert!(value["notes"].is_array());
        Ok(())
    }

    #[tokio::test]
    async fn list_project_notes_forwards_role_and_q() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/workspace/projects/proj/notes"))
            .and(query_param("role", "observer"))
            .and(query_param("q", "stuck"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"notes": []})),
            )
            .mount(&server)
            .await;
        let value = client
            .list_project_notes(
                "proj",
                ProjectNotesQuery {
                    role: Some("observer"),
                    q: Some("stuck"),
                    ..Default::default()
                },
            )
            .await?;
        assert!(value["notes"].is_array());
        Ok(())
    }

    #[tokio::test]
    async fn delete_session_note_204_normalises_to_deleted_marker() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("DELETE"))
            .and(path("/api/sessions/sess-1/notes/n1"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let value = client.delete_session_note("sess-1", "n1").await?;
        assert_eq!(value["deleted"], true);
        Ok(())
    }
}
