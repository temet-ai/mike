use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn eval_runs_get(
        &self,
        path: &str,
        pairs: &[(&str, String)],
    ) -> Result<Value, McpError> {
        self.get_json(path, pairs).await
    }

    pub(crate) async fn eval_runs_get_one(
        &self,
        path: &str,
        resource: &str,
    ) -> Result<Value, McpError> {
        self.get_json_with_not_found(path, resource).await
    }
}
