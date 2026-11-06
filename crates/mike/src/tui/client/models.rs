use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;

use super::ApiClient;

const CATALOGUE_TIMEOUT: Duration = Duration::from_secs(25);

#[derive(Debug, Clone, Deserialize)]
struct FeatureState {
    key: String,
    enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct FeatureStates {
    features: Vec<FeatureState>,
}

#[derive(Debug, Clone, Deserialize)]
struct SettingValue {
    key: String,
    value: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
struct SettingValues {
    settings: Vec<SettingValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProviderModelDto {
    pub id: String,
    #[serde(rename = "ref")]
    pub model_ref: String,
    pub label: String,
    #[serde(default)]
    pub known_good: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProviderDto {
    pub id: String,
    pub label: String,
    pub prefix: String,
    pub available: bool,
    pub detail: String,
    pub fix: String,
    #[serde(default)]
    pub key_env_var: Option<String>,
    #[serde(default)]
    pub models: Vec<ProviderModelDto>,
    #[serde(default)]
    pub models_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProvidersPayload {
    pub llm_provider: String,
    pub providers: Vec<ProviderDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct KeyVerdict {
    pub env_var_name: String,
    pub configured: bool,
    pub valid: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct KeyPutReply {
    valid: bool,
    label: Option<String>,
    error: Option<String>,
}

impl ApiClient {
    pub async fn verify_openrouter_key(&self) -> Option<KeyVerdict> {
        let response = self
            .http
            .post(format!(
                "{}/settings/steerer/openrouter/verify",
                self.inspect_base
            ))
            .timeout(CATALOGUE_TIMEOUT)
            .send()
            .await
            .ok()?;
        response.json().await.ok()
    }

    pub async fn put_openrouter_key(&self, key: &str) -> Result<String, String> {
        let response = self
            .http
            .put(format!(
                "{}/settings/steerer/openrouter/key",
                self.inspect_base
            ))
            .timeout(CATALOGUE_TIMEOUT)
            .json(&serde_json::json!({ "key": key }))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let reply: KeyPutReply = response.json().await.map_err(|e| e.to_string())?;
        if reply.valid {
            Ok(reply.label.unwrap_or_default())
        } else {
            Err(reply
                .error
                .unwrap_or_else(|| "OpenRouter rejected the key".to_string()))
        }
    }

    pub async fn feature_states(&self) -> Option<Vec<(String, bool)>> {
        let payload: FeatureStates = self
            .get_json(&format!("{}/settings/features", self.inspect_base))
            .await?;
        Some(
            payload
                .features
                .into_iter()
                .map(|f| (f.key, f.enabled))
                .collect(),
        )
    }

    pub async fn setting_values(&self) -> Option<HashMap<String, serde_json::Value>> {
        let payload: SettingValues = self
            .get_json(&format!("{}/settings", self.inspect_base))
            .await?;
        Some(
            payload
                .settings
                .into_iter()
                .map(|s| (s.key, s.value))
                .collect(),
        )
    }

    pub async fn llm_providers(&self) -> Option<ProvidersPayload> {
        let response = self
            .http
            .get(format!("{}/llm/providers", self.inspect_base))
            .timeout(CATALOGUE_TIMEOUT)
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.json().await.ok()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FirstRequest {
    pub id: u64,
    pub session_id: String,
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub model: Option<String>,
    pub status: String,
    pub created_at: String,
    #[serde(default)]
    pub project: Option<String>,
}

impl ApiClient {
    pub async fn first_request_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Option<FirstRequest> {
        let from = since.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let url = format!(
            "{}/requests?date_from={from}&order=asc&limit=1",
            self.inspect_base
        );
        let rows: Vec<FirstRequest> = self.get_json(&url).await?;
        rows.into_iter().next()
    }
}
