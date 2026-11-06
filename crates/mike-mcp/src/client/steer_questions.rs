use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn steer_questions_get(
        &self,
        path: &str,
        pairs: &[(&str, String)],
    ) -> Result<Value, McpError> {
        self.get_json(path, pairs).await
    }
}
