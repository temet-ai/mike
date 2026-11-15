use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{Value, json};

use super::decide::Settings;
use super::types::Decision;

const SETTINGS_TIMEOUT: Duration = Duration::from_secs(30);
const HEALTH_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) const POLICY_KEYS: [&str; 6] = [
    "min_bytes",
    "head_lines",
    "tail_lines",
    "grace_turns",
    "enabled_producers",
    "protected_paths",
];

pub(super) const IMPLEMENTATION: &str = "file-read-v3-ranged-recovery";

pub(super) struct InspectApi {
    client: reqwest::Client,
    base: String,
}

impl InspectApi {
    pub(super) fn new(base: &str) -> Result<Self> {
        let client = reqwest::Client::builder()
            .build()
            .context("could not build the inspect API client")?;
        Ok(Self {
            client,
            base: base.trim_end_matches('/').to_string(),
        })
    }

    pub(super) async fn settings(&self) -> Result<Settings> {
        let url = format!("{}/api/settings", self.base);
        let body: Value = self
            .client
            .get(&url)
            .timeout(SETTINGS_TIMEOUT)
            .send()
            .await
            .with_context(|| format!("GET {url} failed"))?
            .error_for_status()
            .with_context(|| format!("GET {url} was refused"))?
            .json()
            .await
            .with_context(|| format!("GET {url} returned a body that is not JSON"))?;
        let rows = body
            .get("settings")
            .and_then(Value::as_array)
            .with_context(|| format!("GET {url} returned no \"settings\" array"))?;
        let map: HashMap<String, Value> = rows
            .iter()
            .filter_map(|row| {
                let key = row.get("key")?.as_str()?.to_string();
                Some((key, row.get("value").cloned().unwrap_or(Value::Null)))
            })
            .collect();
        Ok(Settings(map))
    }

    pub(super) async fn put_setting(&self, decision: &Decision) -> Result<()> {
        let url = format!("{}/api/settings/{}", self.base, decision.key);
        self.client
            .patch(&url)
            .timeout(SETTINGS_TIMEOUT)
            .header("x-change-source", "cli")
            .json(&json!({ "value": decision.value }))
            .send()
            .await
            .with_context(|| format!("PATCH {url} failed"))?
            .error_for_status()
            .with_context(|| format!("PATCH {url} was refused for value {}", decision.value))?;
        Ok(())
    }

    pub(super) async fn running_build(&self) -> Option<String> {
        let url = format!("{}/api/health", self.base);
        let response = self
            .client
            .get(&url)
            .timeout(HEALTH_TIMEOUT)
            .send()
            .await
            .ok()?;
        let body: Value = response.error_for_status().ok()?.json().await.ok()?;
        let build = body.get("build")?;
        if build.get("dirty").and_then(Value::as_bool).unwrap_or(true) {
            return None;
        }
        build.get("sha")?.as_str().map(str::to_string)
    }
}

pub(super) fn policy_from(settings: &Settings, build_sha: Option<String>) -> Value {
    let mut policy = serde_json::Map::new();
    for key in POLICY_KEYS {
        let value = settings
            .get(&format!("file_read_compression.{key}"))
            .cloned()
            .unwrap_or(Value::Null);
        policy.insert(key.to_string(), value);
    }
    policy.insert("implementation".to_string(), json!(IMPLEMENTATION));
    policy.insert(
        "build_sha".to_string(),
        build_sha.map_or(Value::Null, Value::from),
    );
    Value::Object(policy)
}
