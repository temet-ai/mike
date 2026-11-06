use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn what_changed(&self, query: &[(&str, String)]) -> Result<Value, McpError> {
        self.get_json("/api/changes", query).await
    }
}
