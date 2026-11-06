use serde_json::{Value, json};

use super::InspectClient;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn question_post(&self, path: &str, body: &Value) -> Result<Value, McpError> {
        self.post_json(path, body).await
    }

    pub(crate) async fn thread_read(&self, thread_id: &str) -> Result<Value, McpError> {
        self.get_json(&format!("/api/threads/{thread_id}"), &[])
            .await
    }

    pub(crate) async fn thread_resolve(&self, thread_id: &str) -> Result<Value, McpError> {
        self.post_json(&format!("/api/threads/{thread_id}/resolve"), &json!({}))
            .await
    }
}
