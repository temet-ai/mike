use std::time::Duration;

use serde::{Deserialize, Serialize};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    inspect_base: String,
    inspect_addr: String,
    proxy_addr: String,
    spa_addr: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CapabilityState {
    Active,
    Degraded,
    Inert,
    Off,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CapabilityRow {
    pub id: String,
    pub label: String,
    pub state: CapabilityState,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub fix: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CapabilitiesPayload {
    pub capabilities: Vec<CapabilityRow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingKind {
    Bool,
    Int,
    Float,
    String,
    Text,
    StringList,
    Enum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingApplies {
    NewSessions,
    OnRestart,
    NextCycle,
    NextMessage,
}

impl SettingApplies {
    pub fn label(self) -> &'static str {
        match self {
            SettingApplies::NewSessions => "immediate",
            SettingApplies::OnRestart => "applies on restart",
            SettingApplies::NextCycle => "next worker cycle",
            SettingApplies::NextMessage => "next message sent",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SettingEntry {
    pub key: String,
    pub group: String,
    pub kind: SettingKind,
    pub applies: SettingApplies,
    pub description: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SettingsPayload {
    pub settings: Vec<SettingEntry>,
    pub steerer_openrouter_configured_via_env: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeatureEntry {
    pub key: String,
    pub enabled: bool,
    pub applies: SettingApplies,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeaturesPayload {
    pub features: Vec<FeatureEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemoryUsagePayload {
    pub entry_count: u64,
    pub entries_total_bytes: u64,
    pub max_store_bytes: u64,
    pub over_byte_budget: bool,
    pub process_rss_bytes: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub first_seen: String,
    pub request_count: usize,
    pub models: Vec<String>,
    pub project: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    pub busy: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RequestSummary {
    pub id: u64,
    pub method: String,
    pub status: String,
    pub model: Option<String>,
    pub created_at: String,
    pub turn_origin: String,
    pub tool_names: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SessionRequestsPage {
    pub requests: Vec<RequestSummary>,
    pub total_requests: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub enum RequestStatusDto {
    Pending,
    Streaming,
    Complete,
    Error(String),
}

impl std::fmt::Display for RequestStatusDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Streaming => write!(f, "streaming"),
            Self::Complete => write!(f, "complete"),
            Self::Error(e) => write!(f, "error: {e}"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenUsageDto {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_creation_input_tokens: Option<u32>,
    pub cache_read_input_tokens: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RequestDetail {
    pub id: u64,
    pub session_id: String,
    #[serde(default)]
    pub request_number: Option<u64>,
    pub method: String,
    pub path: String,
    pub model: Option<String>,
    pub is_streaming: bool,
    pub message_count: usize,
    pub human_prompt: String,
    pub tool_names: Vec<String>,
    pub project: Option<String>,
    pub status: RequestStatusDto,
    pub request_body_size: u64,
    pub response_body_size: u64,
    pub token_usage: Option<TokenUsageDto>,
    pub stop_reason: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    Request,
    Response,
}

impl BodyKind {
    fn path_segment(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Response => "response",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BodyPayload {
    pub text: Option<String>,
    pub byte_size: u64,
    pub has_body: bool,
}

#[derive(Debug, Serialize)]
struct PatchSettingBody {
    value: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct PatchFeatureBody {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    error: String,
}

impl ApiClient {
    pub fn new(config: &crate::config::ProxyConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            http,
            inspect_base: format!("http://{}/api", config.inspect_listen_addr),
            inspect_addr: config.inspect_listen_addr.clone(),
            proxy_addr: config.proxy_listen_addr.clone(),
            spa_addr: config.spa_enabled.then(|| config.spa_bind.clone()),
        }
    }

    pub async fn bind_states(&self) -> Vec<(&'static str, String, bool)> {
        let mut rows = vec![
            (
                "proxy · /v1",
                self.proxy_addr.clone(),
                tcp_reachable(&self.proxy_addr).await,
            ),
            (
                "inspect · /api",
                self.inspect_addr.clone(),
                self.health().await,
            ),
        ];
        if let Some(spa_addr) = &self.spa_addr {
            rows.push(("web ui", spa_addr.clone(), tcp_reachable(spa_addr).await));
        }
        rows
    }

    pub async fn health(&self) -> bool {
        let Ok(resp) = self
            .http
            .get(format!("{}/health", self.inspect_base))
            .send()
            .await
        else {
            return false;
        };
        resp.status().is_success()
    }

    pub async fn capabilities(&self) -> Option<CapabilitiesPayload> {
        self.get_json(&format!("{}/capabilities", self.inspect_base))
            .await
    }

    pub async fn settings(&self) -> Option<SettingsPayload> {
        self.get_json(&format!("{}/settings", self.inspect_base))
            .await
    }

    pub async fn features(&self) -> Option<FeaturesPayload> {
        self.get_json(&format!("{}/settings/features", self.inspect_base))
            .await
    }

    pub async fn memory_usage(&self) -> Option<MemoryUsagePayload> {
        self.get_json(&format!("{}/diagnostics/memory", self.inspect_base))
            .await
    }

    pub async fn list_sessions(&self, offset: usize, limit: usize) -> Option<Vec<SessionSummary>> {
        self.get_json(&format!(
            "{}/sessions?offset={offset}&limit={limit}",
            self.inspect_base
        ))
        .await
    }

    pub async fn session_requests(
        &self,
        session_id: &str,
        offset: usize,
        limit: usize,
    ) -> Option<SessionRequestsPage> {
        self.get_json(&format!(
            "{}/sessions/{session_id}/requests?offset={offset}&limit={limit}",
            self.inspect_base
        ))
        .await
    }

    pub async fn request_detail(&self, id: u64) -> Option<RequestDetail> {
        self.get_json(&format!("{}/requests/{id}", self.inspect_base))
            .await
    }

    pub async fn request_body(&self, id: u64, kind: BodyKind) -> Option<BodyPayload> {
        self.get_json(&format!(
            "{}/requests/{id}/body/{}",
            self.inspect_base,
            kind.path_segment()
        ))
        .await
    }

    pub async fn patch_setting(&self, key: &str, value: serde_json::Value) -> Result<(), String> {
        let url = format!("{}/settings/{key}", self.inspect_base);
        let resp = self
            .http
            .patch(&url)
            .json(&PatchSettingBody { value })
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if resp.status().is_success() {
            return Ok(());
        }
        Err(Self::extract_error(resp).await)
    }

    pub async fn patch_feature(&self, key: &str, enabled: bool) -> Result<(), String> {
        let url = format!("{}/settings/features/{key}", self.inspect_base);
        let resp = self
            .http
            .patch(&url)
            .json(&PatchFeatureBody { enabled })
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if resp.status().is_success() {
            return Ok(());
        }
        Err(Self::extract_error(resp).await)
    }

    async fn extract_error(resp: reqwest::Response) -> String {
        let status = resp.status();
        match resp.json::<ErrorBody>().await {
            Ok(body) => body.error,
            Err(_) => format!("request failed: {status}"),
        }
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Option<T> {
        let resp = self.http.get(url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        resp.json::<T>().await.ok()
    }
}

pub(crate) async fn tcp_reachable(addr: &str) -> bool {
    tokio::time::timeout(REQUEST_TIMEOUT, tokio::net::TcpStream::connect(addr))
        .await
        .is_ok_and(|r| r.is_ok())
}

mod models;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub use models::ProviderModelDto;
pub use models::{FirstRequest, ProviderDto, ProvidersPayload};
