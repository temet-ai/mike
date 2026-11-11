use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn supplied_items_get(
        &self,
        pairs: &[(&str, String)],
    ) -> Result<Value, McpError> {
        self.get_json("/api/supplied-items", pairs).await
    }
}
