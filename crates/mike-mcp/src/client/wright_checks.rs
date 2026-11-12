use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn wright_checks_list(&self) -> Result<Value, McpError> {
        self.get_json("/api/wright/checks", &[]).await
    }

    pub(crate) async fn wright_checks_add(&self, body: Value) -> Result<Value, McpError> {
        self.post_json("/api/wright/checks", &body).await
    }

    pub(crate) async fn wright_checks_update(
        &self,
        name: &str,
        body: Value,
    ) -> Result<Value, McpError> {
        let path = format!("/api/wright/checks/{name}");
        self.patch_json_with_not_found(&path, &body, &format!("Wright check {name}"))
            .await
    }

    pub(crate) async fn wright_checks_remove(&self, name: &str) -> Result<Value, McpError> {
        let path = format!("/api/wright/checks/{name}");
        self.delete_json_with_not_found(&path, &format!("Wright check {name}"))
            .await
    }
}
