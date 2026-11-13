pub mod insert;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    NotCollected,
    Retained,
    BudgetRefused,
    Incomplete,
    Expired,
}

impl EvidenceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotCollected => "not_collected",
            Self::Retained => "retained",
            Self::BudgetRefused => "budget_refused",
            Self::Incomplete => "incomplete",
            Self::Expired => "expired",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiagnosticLimits {
    pub enabled: bool,
    pub max_record_bytes: usize,
    pub max_total_bytes: i64,
    pub max_session_bytes: i64,
    pub ttl_seconds: i64,
}

impl Default for DiagnosticLimits {
    fn default() -> Self {
        Self {
            enabled: false,
            max_record_bytes: 4 * 1024 * 1024,
            max_total_bytes: 256 * 1024 * 1024,
            max_session_bytes: 32 * 1024 * 1024,
            ttl_seconds: 86_400,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticExchange {
    pub session_id: Option<Uuid>,
    pub provider_session_id: Option<String>,
    pub provider_thread_id: Option<String>,
    pub provider_turn_id: Option<String>,
    pub request_kind: Option<String>,
    pub correlation: Value,
    pub auth_route: String,
    pub transport: String,
    pub method: String,
    pub path: String,
    pub upstream_url: String,
    pub request_headers: Value,
    pub forwarded_headers: Value,
    #[serde(skip_serializing)]
    pub request_body: Option<Vec<u8>>,
    #[serde(skip_serializing)]
    pub forwarded_body: Option<Vec<u8>>,
    pub request_body_encoding: Option<String>,
    pub model: Option<String>,
    pub response_status: Option<i16>,
    pub response_headers: Option<Value>,
    #[serde(skip_serializing)]
    pub response_body: Option<Vec<u8>>,
    pub upstream_request_id: Option<String>,
    pub response_id: Option<String>,
    pub response_model: Option<String>,
    pub response_state: Option<String>,
    pub usage: Option<Value>,
    pub stream_events: Option<i32>,
    pub error: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub evidence_status: EvidenceStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticOutcome {
    pub exchange_id: Option<i64>,
    pub evidence_status: EvidenceStatus,
}

impl crate::store::persistence::runtime_repo::RuntimeDb<'_> {
    pub async fn record_responses_diagnostic(
        self,
        exchange: &DiagnosticExchange,
        limits: &DiagnosticLimits,
    ) -> Result<DiagnosticOutcome, PersistError> {
        insert::record(&self, exchange, limits).await
    }
}
