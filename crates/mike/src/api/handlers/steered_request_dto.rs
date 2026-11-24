use super::*;

#[derive(Serialize)]
pub(crate) struct SteeredRequestDto {
    #[serde(rename = "sent_request_id")]
    pub(crate) request_id: i64,
    #[serde(rename = "sent_response_id")]
    pub(crate) response_id: Option<i64>,
    pub(crate) session_uuid: uuid::Uuid,
    pub(crate) operator_turn_id: Option<i64>,
    pub(crate) steerer_name: String,
    pub(crate) steerer_display_name: String,
    #[serde(rename = "steerer_model")]
    pub(crate) model: Option<String>,
    pub(crate) authority: String,
    pub(crate) mode: String,
    pub(crate) verdict: String,
    pub(crate) confidence: Option<f64>,
    pub(crate) off_thread_topic: Option<String>,
    pub(crate) anchor_text: Option<String>,
    pub(crate) payload_text: Option<String>,
    pub(crate) verdict_json: Option<serde_json::Value>,
    pub(crate) trigger_request_id: Option<i64>,
    pub(crate) sent_at: Option<String>,
    pub(crate) processed_at: Option<String>,
    pub(crate) expires_at: Option<String>,
    pub(crate) reverted_at: Option<String>,
    pub(crate) revert_reason: Option<String>,
    pub(crate) created_at: String,
}
