use serde::Deserialize;
use serde_json::{Value, json};

use super::InspectClient;
use crate::error::McpError;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ClaimedChannelMessage {
    pub(crate) message_uuid: String,
    pub(crate) from_session_uuid: Option<String>,
    pub(crate) from_display_name: Option<String>,
    pub(crate) from_role: Option<String>,
    pub(crate) envelope: String,
    #[serde(default)]
    pub(crate) body: String,
    #[serde(default)]
    pub(crate) from_peer_address: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ChannelClaimResponse {
    #[serde(default)]
    pub(crate) native_delivery: bool,
    pub(crate) messages: Vec<ClaimedChannelMessage>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ClaimRoutes<'a> {
    pub(crate) peer_address: Option<&'a str>,
    pub(crate) channel: bool,
}

impl ClaimRoutes<'_> {
    fn request_body(&self) -> Value {
        let mut body = serde_json::Map::new();
        body.insert("channel".into(), Value::Bool(self.channel));
        if let Some(address) = self.peer_address {
            body.insert("peer_address".into(), Value::String(address.to_string()));
        }
        Value::Object(body)
    }
}

impl InspectClient {
    pub(crate) async fn claim_channel_messages(
        &self,
        session_uuid: &str,
        routes: ClaimRoutes<'_>,
    ) -> Result<ChannelClaimResponse, McpError> {
        let path = format!("/api/workspace/sessions/{session_uuid}/channel/claim");
        let value = self.post_json(&path, &routes.request_body()).await?;
        serde_json::from_value(value)
            .map_err(|e| McpError::Parse(format!("channel claim response: {e}")))
    }

    pub(crate) async fn release_channel_messages(
        &self,
        session_uuid: &str,
        message_uuids: &[String],
    ) -> Result<(), McpError> {
        let path = format!("/api/workspace/sessions/{session_uuid}/channel/release");
        self.post_json(&path, &json!({ "message_uuids": message_uuids }))
            .await
            .map(|_| ())
    }
}
