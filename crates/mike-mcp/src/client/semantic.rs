use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn get_semantic_status(&self) -> Result<Value, McpError> {
        self.get_json("/api/semantic/status", &[]).await
    }

    pub(crate) async fn get_semantic_neighbours(
        &self,
        turn_id: i64,
        pairs: &[(&str, String)],
    ) -> Result<Value, McpError> {
        self.get_json(&format!("/api/semantic/turns/{turn_id}/neighbours"), pairs)
            .await
    }

    pub(crate) async fn get_semantic_item(&self, id: i64) -> Result<Value, McpError> {
        self.get_json(&format!("/api/semantic/items/{id}"), &[])
            .await
    }

    pub(crate) async fn list_pattern_assignments(
        &self,
        pairs: &[(&str, String)],
    ) -> Result<Value, McpError> {
        self.get_json("/api/semantic/patterns/assignments", pairs)
            .await
    }

    pub(crate) async fn list_pattern_candidates(
        &self,
        pairs: &[(&str, String)],
    ) -> Result<Value, McpError> {
        self.get_json("/api/semantic/patterns/candidates", pairs)
            .await
    }

    pub(crate) async fn get_embedding_drift(
        &self,
        pairs: &[(&str, String)],
    ) -> Result<Value, McpError> {
        self.get_json("/api/semantic/drift", pairs).await
    }
}
