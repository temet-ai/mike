use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn get_session_dossier(
        &self,
        session_id: &str,
        query: &[(&str, String)],
    ) -> Result<Value, McpError> {
        self.get_json_with_not_found_query(
            &format!("/api/sessions/{session_id}/dossier"),
            query,
            &format!("session {session_id}"),
        )
        .await
    }

    pub(crate) async fn get_turn_facts(&self, turn_id: i64) -> Result<Value, McpError> {
        self.get_json_with_not_found(
            &format!("/api/operator-turns/{turn_id}/facts"),
            &format!("operator turn {turn_id}"),
        )
        .await
    }
}
