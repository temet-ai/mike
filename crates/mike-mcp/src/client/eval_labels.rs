use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn relay_label(&self, body: &Value) -> Result<Value, McpError> {
        self.post_json("/api/eval/labels/relay", body).await
    }

    pub(crate) async fn list_eval_labels(
        &self,
        query: &[(&str, String)],
    ) -> Result<Value, McpError> {
        self.get_json("/api/eval/labels", query).await
    }
}
