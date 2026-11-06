use reqwest::StatusCode;
use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

pub(crate) struct PagedJson {
    pub(crate) body: Value,
    pub(crate) window_note: Option<String>,
    pub(crate) next_cursor: Option<String>,
}

impl InspectClient {
    pub(crate) async fn get_json(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<Value, McpError> {
        let url = format!("{}{path}", self.base_url);
        let response = self
            .client
            .get(&url)
            .query(query)
            .send()
            .await
            .map_err(|e| {
                McpError::Unreachable(format!(
                    "GET {path} failed: {e} (is the proxy running at {}?)",
                    self.base_url
                ))
            })?;
        let status = response.status();
        if status.is_success() {
            self.read_capped_json(response, path).await
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(McpError::UpstreamStatus {
                status: status.as_u16(),
                body,
            })
        }
    }

    pub(crate) async fn get_json_with_note(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<(Value, Option<String>), McpError> {
        let page = self.get_json_paged(path, query).await?;
        Ok((page.body, page.window_note))
    }

    pub(crate) async fn get_json_paged(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<PagedJson, McpError> {
        let url = format!("{}{path}", self.base_url);
        let response = self
            .client
            .get(&url)
            .query(query)
            .send()
            .await
            .map_err(|e| {
                McpError::Unreachable(format!(
                    "GET {path} failed: {e} (is the proxy running at {}?)",
                    self.base_url
                ))
            })?;
        let status = response.status();
        if status.is_success() {
            let header = |name: &str| {
                response
                    .headers()
                    .get(name)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string())
            };
            let window_note = header("x-window-note");
            let next_cursor = header("x-next-cursor");
            let body = self.read_capped_json(response, path).await?;
            Ok(PagedJson {
                body,
                window_note,
                next_cursor,
            })
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(McpError::UpstreamStatus {
                status: status.as_u16(),
                body,
            })
        }
    }

    pub(crate) async fn get_json_with_not_found(
        &self,
        path: &str,
        resource: &str,
    ) -> Result<Value, McpError> {
        self.get_json_with_not_found_query(path, &[], resource)
            .await
    }

    pub(crate) async fn get_json_with_not_found_query(
        &self,
        path: &str,
        query: &[(&str, String)],
        resource: &str,
    ) -> Result<Value, McpError> {
        let url = format!("{}{path}", self.base_url);
        let response = self
            .client
            .get(&url)
            .query(query)
            .send()
            .await
            .map_err(|e| {
                McpError::Unreachable(format!(
                    "GET {path} failed: {e} (is the proxy running at {}?)",
                    self.base_url
                ))
            })?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Err(McpError::NotFound(resource.to_string()));
        }
        if status.is_success() {
            self.read_capped_json(response, path).await
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(McpError::UpstreamStatus {
                status: status.as_u16(),
                body,
            })
        }
    }

    pub(crate) async fn get_json_full_url(
        &self,
        url: reqwest::Url,
        resource: &str,
    ) -> Result<Value, McpError> {
        let path = url.path().to_string();
        let response = self.client.get(url).send().await.map_err(|e| {
            McpError::Unreachable(format!(
                "GET {path} failed: {e} (is the proxy running at {}?)",
                self.base_url
            ))
        })?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Err(McpError::NotFound(resource.to_string()));
        }
        if status.is_success() {
            self.read_capped_json(response, &path).await
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(McpError::UpstreamStatus {
                status: status.as_u16(),
                body,
            })
        }
    }

    pub(crate) async fn post_json(&self, path: &str, body: &Value) -> Result<Value, McpError> {
        let url = format!("{}{path}", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(|e| {
                McpError::Unreachable(format!(
                    "POST {path} failed: {e} (is the proxy running at {}?)",
                    self.base_url
                ))
            })?;
        let status = response.status();
        if status.is_success() {
            self.read_capped_json(response, path).await
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(McpError::UpstreamStatus {
                status: status.as_u16(),
                body,
            })
        }
    }

    pub(crate) async fn patch_json_with_not_found(
        &self,
        path: &str,
        body: &Value,
        resource: &str,
    ) -> Result<Value, McpError> {
        let url = format!("{}{path}", self.base_url);
        let response = self
            .client
            .patch(&url)
            .json(body)
            .send()
            .await
            .map_err(|e| {
                McpError::Unreachable(format!(
                    "PATCH {path} failed: {e} (is the proxy running at {}?)",
                    self.base_url
                ))
            })?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Err(McpError::NotFound(resource.to_string()));
        }
        if status.is_success() {
            self.read_capped_json(response, path).await
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(McpError::UpstreamStatus {
                status: status.as_u16(),
                body,
            })
        }
    }

    pub(crate) async fn delete_json_with_not_found(
        &self,
        path: &str,
        resource: &str,
    ) -> Result<Value, McpError> {
        let url = format!("{}{path}", self.base_url);
        let response = self.client.delete(&url).send().await.map_err(|e| {
            McpError::Unreachable(format!(
                "DELETE {path} failed: {e} (is the proxy running at {}?)",
                self.base_url
            ))
        })?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Err(McpError::NotFound(resource.to_string()));
        }
        if status == StatusCode::NO_CONTENT {
            return Ok(serde_json::json!({ "deleted": true, "warnings": [] }));
        }
        if status.is_success() {
            let mut value = self.read_capped_json(response, path).await?;
            if let Some(obj) = value.as_object_mut() {
                obj.entry("deleted".to_string())
                    .or_insert(Value::Bool(true));
            }
            Ok(value)
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(McpError::UpstreamStatus {
                status: status.as_u16(),
                body,
            })
        }
    }

    pub(crate) async fn put_json_with_not_found(
        &self,
        path: &str,
        body: &Value,
        resource: &str,
    ) -> Result<Value, McpError> {
        let url = format!("{}{path}", self.base_url);
        let response = self.client.put(&url).json(body).send().await.map_err(|e| {
            McpError::Unreachable(format!(
                "PUT {path} failed: {e} (is the proxy running at {}?)",
                self.base_url
            ))
        })?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Err(McpError::NotFound(resource.to_string()));
        }
        if status.is_success() {
            self.read_capped_json(response, path).await
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(McpError::UpstreamStatus {
                status: status.as_u16(),
                body,
            })
        }
    }

    pub(crate) async fn post_json_with_not_found(
        &self,
        path: &str,
        body: &Value,
        resource: &str,
    ) -> Result<Value, McpError> {
        let url = format!("{}{path}", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .map_err(|e| {
                McpError::Unreachable(format!(
                    "POST {path} failed: {e} (is the proxy running at {}?)",
                    self.base_url
                ))
            })?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Err(McpError::NotFound(resource.to_string()));
        }
        if status.is_success() {
            self.read_capped_json(response, path).await
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(McpError::UpstreamStatus {
                status: status.as_u16(),
                body,
            })
        }
    }

    pub(crate) async fn post_empty_with_not_found(
        &self,
        path: &str,
        resource: &str,
    ) -> Result<Value, McpError> {
        let url = format!("{}{path}", self.base_url);
        let response = self.client.post(&url).send().await.map_err(|e| {
            McpError::Unreachable(format!(
                "POST {path} failed: {e} (is the proxy running at {}?)",
                self.base_url
            ))
        })?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Err(McpError::NotFound(resource.to_string()));
        }
        if status == StatusCode::NO_CONTENT {
            return Ok(serde_json::json!({ "ended": true }));
        }
        if status.is_success() {
            self.read_capped_json(response, path).await
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(McpError::UpstreamStatus {
                status: status.as_u16(),
                body,
            })
        }
    }
}

pub(crate) fn body_pointer_query(pointer: Option<&str>) -> Vec<(&'static str, String)> {
    let mut query = Vec::with_capacity(1);
    if let Some(pointer) = pointer {
        query.push(("pointer", pointer.to_string()));
    }
    query
}

fn build_encoded_project_path(
    prefix: &[&str],
    project_path: &str,
    suffix: &[&str],
) -> Result<String, McpError> {
    let mut url = reqwest::Url::parse("http://x")
        .map_err(|e| McpError::Parse(format!("bad dummy base: {e}")))?;
    url.path_segments_mut()
        .map_err(|_| McpError::Parse("dummy url cannot be a base".to_string()))?
        .extend(prefix)
        .push(project_path)
        .extend(suffix);
    Ok(url.path().to_string())
}

pub(crate) fn build_workspace_roster_path(project_path: &str) -> Result<String, McpError> {
    build_encoded_project_path(&["api", "workspace", "projects"], project_path, &["roster"])
}

pub(crate) fn build_workspace_notes_path(project_path: &str) -> Result<String, McpError> {
    build_encoded_project_path(&["api", "workspace", "projects"], project_path, &["notes"])
}

pub(crate) fn build_thread_path(slug: &str, suffix: &[&str]) -> Result<String, McpError> {
    build_encoded_project_path(&["api", "work-threads"], slug, suffix)
}

pub(crate) fn build_segments_url(base: &str, segments: &[&str]) -> Result<reqwest::Url, McpError> {
    let mut url =
        reqwest::Url::parse(base).map_err(|e| McpError::Parse(format!("bad base url: {e}")))?;
    url.path_segments_mut()
        .map_err(|_| McpError::Parse("base url cannot be a base".to_string()))?
        .extend(segments);
    Ok(url)
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
    async fn unreachable_base_url_maps_to_unreachable_error() -> anyhow::Result<()> {
        let client = InspectClient::new("http://127.0.0.1:1".to_string());
        let err = client
            .list_requests(None, None, None, None, None)
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("expected Unreachable, got Ok"))?;
        match err {
            McpError::Unreachable(msg) => {
                assert!(msg.contains("127.0.0.1:1"));
            }
            other => anyhow::bail!("expected Unreachable, got {other:?}"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn upstream_500_maps_to_upstream_status_error() -> anyhow::Result<()> {
        let (server, client) = start_mock().await;
        Mock::given(method("GET"))
            .and(path("/api/stats"))
            .respond_with(ResponseTemplate::new(500).set_body_string("oops"))
            .mount(&server)
            .await;

        let err = client
            .stats()
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("expected UpstreamStatus, got Ok"))?;
        match err {
            McpError::UpstreamStatus { status, body } => {
                assert_eq!(status, 500);
                assert!(body.contains("oops"));
            }
            other => anyhow::bail!("expected UpstreamStatus, got {other:?}"),
        }
        Ok(())
    }
}
