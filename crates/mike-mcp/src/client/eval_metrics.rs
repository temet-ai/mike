use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn get_eval_rates(&self, pairs: &[(&str, String)]) -> Result<Value, McpError> {
        self.get_json("/api/eval/rates", pairs).await
    }

    pub(crate) async fn list_eval_alarms(
        &self,
        pairs: &[(&str, String)],
    ) -> Result<Value, McpError> {
        self.get_json("/api/eval/alarms", pairs).await
    }
}
