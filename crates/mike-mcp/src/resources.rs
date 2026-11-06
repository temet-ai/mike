use rmcp::model::{ErrorData as RmcpError, ReadResourceResult, Resource, ResourceContents};
use serde_json::{Value, json};

use crate::client::InspectClient;
use crate::error::McpError;

pub const URI_STATUS: &str = "proxy://status";
pub const URI_REQUESTS: &str = "proxy://requests";
pub const URI_SESSIONS: &str = "proxy://sessions";
pub const URI_REQUESTS_PREFIX: &str = "proxy://requests/";
pub const URI_SESSIONS_PREFIX: &str = "proxy://sessions/";

pub fn list_resources() -> Vec<Resource> {
    vec![
        make_resource(URI_STATUS, "status", Some("Proxy health and runtime stats")),
        make_resource(
            URI_REQUESTS,
            "requests",
            Some("List of stored Claude API requests (top 100, newest first)"),
        ),
        make_resource(
            URI_SESSIONS,
            "sessions",
            Some("List of sessions in the proxy store"),
        ),
    ]
}

fn make_resource(uri: &str, name: &str, description: Option<&str>) -> Resource {
    let r = Resource::new(uri, name).with_mime_type("application/json");
    match description {
        Some(desc) => r.with_description(desc),
        None => r,
    }
}

pub(crate) async fn read_resource(
    client: &InspectClient,
    uri: &str,
) -> Result<ReadResourceResult, RmcpError> {
    let contents = match uri {
        URI_STATUS => fetch_status(client).await,
        URI_REQUESTS => fetch_requests(client).await,
        URI_SESSIONS => fetch_sessions(client).await,
        other => {
            if let Some(rest) = other.strip_prefix(URI_REQUESTS_PREFIX) {
                fetch_request_detail(client, rest).await
            } else if let Some(rest) = other.strip_prefix(URI_SESSIONS_PREFIX) {
                fetch_session_detail(client, rest).await
            } else {
                Err(McpError::NotFound(format!("unknown resource uri: {other}")))
            }
        }
    };

    match contents {
        Ok(value) => Ok(ReadResourceResult::new(vec![
            ResourceContents::TextResourceContents {
                uri: uri.to_string(),
                mime_type: Some("application/json".to_string()),
                text: serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()),
                meta: None,
            },
        ])),
        Err(err) => Err(err.into_rmcp()),
    }
}

async fn fetch_status(client: &InspectClient) -> Result<Value, McpError> {
    let health = client.health().await?;
    let stats = client.stats().await?;
    Ok(json!({
        "health": health,
        "stats": stats,
    }))
}

async fn fetch_requests(client: &InspectClient) -> Result<Value, McpError> {
    client
        .list_requests(None, Some(100), None, None, None)
        .await
}

async fn fetch_sessions(client: &InspectClient) -> Result<Value, McpError> {
    client.list_sessions(None, None, None).await
}

async fn fetch_request_detail(client: &InspectClient, id_str: &str) -> Result<Value, McpError> {
    let id: u64 = id_str
        .parse()
        .map_err(|_| McpError::NotFound(format!("invalid request id: {id_str}")))?;
    client.get_request(id).await
}

async fn fetch_session_detail(client: &InspectClient, id: &str) -> Result<Value, McpError> {
    if id.is_empty() {
        return Err(McpError::NotFound("empty session id".to_string()));
    }
    client.get_session(id, None, None, None).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_resources_has_three_static_entries() {
        let resources = list_resources();
        assert_eq!(resources.len(), 3);
        let uris: Vec<&str> = resources.iter().map(|r| r.uri.as_str()).collect();
        assert!(uris.contains(&URI_STATUS));
        assert!(uris.contains(&URI_REQUESTS));
        assert!(uris.contains(&URI_SESSIONS));
    }

    #[test]
    fn list_resources_all_have_application_json_mime() {
        for r in list_resources() {
            assert_eq!(r.mime_type.as_deref(), Some("application/json"));
        }
    }

    #[test]
    fn uri_prefix_matches_constants_shape() {
        assert!(URI_REQUESTS_PREFIX.starts_with(URI_REQUESTS));
        assert!(URI_REQUESTS_PREFIX.ends_with('/'));
        assert!(URI_SESSIONS_PREFIX.starts_with(URI_SESSIONS));
        assert!(URI_SESSIONS_PREFIX.ends_with('/'));
    }
}
