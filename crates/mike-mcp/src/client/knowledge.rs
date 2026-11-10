use super::InspectClient;
use crate::error::McpError;
use serde_json::{Map, Value};

impl InspectClient {
    pub(crate) async fn knowledge_request(
        &self,
        name: &str,
        args: &Map<String, Value>,
    ) -> Result<Value, McpError> {
        if name == "define_project" {
            return self
                .post_json("/api/knowledge/projects", &Value::Object(args.clone()))
                .await;
        }
        if name == "register_project_location" {
            return self
                .post_json("/api/knowledge/locations", &Value::Object(args.clone()))
                .await;
        }
        let path = match name {
            "list_project_locations" => "/api/knowledge/locations",
            "search_project_knowledge" => "/api/knowledge/search",
            "read_project_knowledge" => "/api/knowledge/read",
            _ => return Err(McpError::Parse(format!("Unknown knowledge tool: {name}"))),
        };
        let default_detail = (name == "search_project_knowledge" && !args.contains_key("detail"))
            .then(|| ("detail", super::DEFAULT_DETAIL.to_string()));
        let query: Vec<(&str, String)> = args
            .iter()
            .map(|(key, value)| {
                (
                    key.as_str(),
                    value
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| value.to_string()),
                )
            })
            .chain(default_detail)
            .collect();
        self.get_json(path, &query).await
    }
}
