use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

const CACHE_TTL: Duration = Duration::from_secs(3600);

#[derive(Debug, Deserialize)]
struct OpenRouterPricing {
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    completion: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterArchitecture {
    #[serde(default)]
    input_modalities: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModel {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    context_length: Option<i64>,
    #[serde(default)]
    pricing: Option<OpenRouterPricing>,
    #[serde(default)]
    architecture: Option<OpenRouterArchitecture>,
    #[serde(default)]
    supported_parameters: Vec<String>,
    #[serde(default)]
    created: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModelsResponse {
    data: Vec<OpenRouterModel>,
}

const CATALOGUE_CUTOFF_UNIX: i64 = 1_767_225_600;

const KNOWN_GOOD_MODEL_PATTERNS: &[&str] = &[
    "google/gemini-3.5-flash-lite",
    "google/gemini-3.6-flash",
    "google/gemini-3.7-flash",
    "google/gemini-3.8-flash",
    "openai/gpt-5.4-mini",
    "openai/gpt-5.4-nano",
    "openai/gpt-5.6-luna",
    "deepseek/deepseek-v4.1-flash",
    "qwen/qwen3.8-flash",
    "z-ai/glm-5.3-flash",
];

const KNOWN_GOOD_EXCLUDE: &[&str] = &[
    "image",
    "deep-research",
    "audio",
    "tts",
    "realtime",
    "search",
    ":batch",
    ":free",
    "-image",
];

fn is_known_good(id: &str) -> bool {
    let id_lower = id.to_ascii_lowercase();
    if KNOWN_GOOD_EXCLUDE
        .iter()
        .any(|x| id_lower.contains(&x.to_ascii_lowercase()))
    {
        return false;
    }
    KNOWN_GOOD_MODEL_PATTERNS
        .iter()
        .any(|p| id_lower.contains(&p.to_ascii_lowercase()))
}

fn released_in_cutoff_window(created: Option<i64>) -> bool {
    created.is_some_and(|c| c >= CATALOGUE_CUTOFF_UNIX)
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub context_length: Option<i64>,
    pub prompt_price_per_1m: f64,
    pub completion_price_per_1m: f64,
    pub input_modalities: Vec<String>,
    pub created_at: Option<i64>,
    pub known_good: bool,
    pub supports_structured_output: bool,
    pub supports_tools: bool,
    pub supports_reasoning: bool,
}

impl From<OpenRouterModel> for ModelEntry {
    fn from(m: OpenRouterModel) -> Self {
        let (prompt_price_per_1m, completion_price_per_1m) = m.pricing.map_or((0.0, 0.0), |p| {
            let prompt = p
                .prompt
                .as_deref()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0)
                * 1_000_000.0;
            let completion = p
                .completion
                .as_deref()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0)
                * 1_000_000.0;
            (prompt, completion)
        });
        let supports_structured_output = m
            .supported_parameters
            .iter()
            .any(|p| p == "structured_outputs" || p == "response_format");
        let known_good = supports_structured_output && is_known_good(&m.id);
        let supports_tools = m.supported_parameters.iter().any(|p| p == "tools");
        let supports_reasoning = m
            .supported_parameters
            .iter()
            .any(|p| p == "reasoning" || p == "include_reasoning");
        let input_modalities = m
            .architecture
            .map(|a| a.input_modalities)
            .unwrap_or_default();
        Self {
            id: m.id,
            name: m.name,
            description: m.description,
            context_length: m.context_length,
            prompt_price_per_1m,
            completion_price_per_1m,
            input_modalities,
            created_at: m.created,
            known_good,
            supports_structured_output,
            supports_tools,
            supports_reasoning,
        }
    }
}

struct CacheEntry {
    entries: Vec<ModelEntry>,
    fetched_at: Instant,
}

pub struct ModelCatalogue {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    cache: RwLock<Option<CacheEntry>>,
}

impl ModelCatalogue {
    pub fn new(client: reqwest::Client, base_url: String, api_key: Option<String>) -> Self {
        Self {
            client,
            base_url,
            api_key,
            cache: RwLock::new(None),
        }
    }

    pub async fn bust_cache(&self) {
        let mut guard = self.cache.write().await;
        *guard = None;
    }

    pub async fn get(&self) -> anyhow::Result<Vec<ModelEntry>> {
        {
            let guard = self.cache.read().await;
            if let Some(entry) = guard.as_ref()
                && entry.fetched_at.elapsed() < CACHE_TTL
            {
                return Ok(entry.entries.clone());
            }
        }

        let mut guard = self.cache.write().await;
        if let Some(entry) = guard.as_ref()
            && entry.fetched_at.elapsed() < CACHE_TTL
        {
            return Ok(entry.entries.clone());
        }

        let url = format!("{}/models", self.base_url);
        let mut req = self.client.get(&url);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("OpenRouter /models request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(anyhow::anyhow!(
                "OpenRouter /models returned non-2xx status: {status}"
            ));
        }

        let or_resp: OpenRouterModelsResponse = resp.json().await.map_err(|e| {
            anyhow::anyhow!("failed to deserialize OpenRouter /models response: {e}")
        })?;

        let entries: Vec<ModelEntry> = or_resp
            .data
            .into_iter()
            .filter(|m| released_in_cutoff_window(m.created))
            .map(ModelEntry::from)
            .collect();

        *guard = Some(CacheEntry {
            entries: entries.clone(),
            fetched_at: Instant::now(),
        });

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const CREATED_2026: i64 = 1_780_000_000;
    const STRUCTURED: &[&str] = &["structured_outputs"];

    fn model_json(id: &str, created: Option<i64>, params: &[&str]) -> serde_json::Value {
        let mut v = json!({
            "id": id,
            "name": id,
            "supported_parameters": params,
        });
        if let Some(c) = created {
            v["created"] = json!(c);
        }
        v
    }

    fn entry_from(value: serde_json::Value) -> anyhow::Result<ModelEntry> {
        let model: OpenRouterModel = serde_json::from_value(value)?;
        Ok(ModelEntry::from(model))
    }

    async fn catalogue_serving(models: Vec<serde_json::Value>) -> (MockServer, ModelCatalogue) {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": models })))
            .mount(&server)
            .await;
        let catalogue = ModelCatalogue::new(reqwest::Client::new(), server.uri(), None);
        (server, catalogue)
    }

    async fn served_ids(models: Vec<serde_json::Value>) -> anyhow::Result<Vec<String>> {
        let (_server, catalogue) = catalogue_serving(models).await;
        Ok(catalogue.get().await?.into_iter().map(|e| e.id).collect())
    }

    #[tokio::test]
    async fn get_drops_models_created_before_2026() -> anyhow::Result<()> {
        let ids = served_ids(vec![
            model_json("openai/gpt-4o-mini", Some(1_721_260_800), STRUCTURED),
            model_json("openai/gpt-5.6-luna", Some(CREATED_2026), STRUCTURED),
        ])
        .await?;

        assert_eq!(ids, vec!["openai/gpt-5.6-luna".to_owned()]);
        Ok(())
    }

    #[tokio::test]
    async fn get_drops_models_without_created() -> anyhow::Result<()> {
        let ids = served_ids(vec![
            model_json("google/gemini-3.8-flash", None, STRUCTURED),
            model_json("google/gemini-3.7-flash", Some(CREATED_2026), STRUCTURED),
        ])
        .await?;

        assert_eq!(ids, vec!["google/gemini-3.7-flash".to_owned()]);
        Ok(())
    }

    #[tokio::test]
    async fn get_keeps_model_created_exactly_at_cutoff() -> anyhow::Result<()> {
        let (_server, catalogue) = catalogue_serving(vec![model_json(
            "google/gemini-3.6-flash",
            Some(CATALOGUE_CUTOFF_UNIX),
            STRUCTURED,
        )])
        .await;

        let entries = catalogue.get().await?;

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].created_at, Some(CATALOGUE_CUTOFF_UNIX));
        Ok(())
    }

    #[tokio::test]
    async fn get_drops_model_created_one_second_before_cutoff() -> anyhow::Result<()> {
        let ids = served_ids(vec![model_json(
            "google/gemini-3.6-flash",
            Some(CATALOGUE_CUTOFF_UNIX - 1),
            STRUCTURED,
        )])
        .await?;

        assert!(ids.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn get_propagates_non_success_status() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let catalogue = ModelCatalogue::new(reqwest::Client::new(), server.uri(), None);

        assert!(catalogue.get().await.is_err());
    }

    #[test]
    fn allowlisted_models_are_known_good() -> anyhow::Result<()> {
        for id in KNOWN_GOOD_MODEL_PATTERNS {
            let entry = entry_from(model_json(id, Some(CREATED_2026), STRUCTURED))?;
            assert!(entry.known_good, "{id} should be known good");
        }
        Ok(())
    }

    #[test]
    fn retired_models_are_not_known_good() -> anyhow::Result<()> {
        for id in [
            "google/gemini-2.5-flash-lite",
            "google/gemini-3-flash-preview",
            "openai/gpt-4o-mini",
            "openai/gpt-5-mini",
            "openai/gpt-4.1-mini",
            "openai/o4-mini",
            "anthropic/claude-3.5-haiku",
            "anthropic/claude-haiku-4.5",
            "moonshotai/kimi-k3",
        ] {
            let entry = entry_from(model_json(id, Some(CREATED_2026), STRUCTURED))?;
            assert!(!entry.known_good, "{id} should not be known good");
        }
        Ok(())
    }

    #[test]
    fn excluded_suffixes_are_not_known_good() -> anyhow::Result<()> {
        for id in [
            "google/gemini-3.8-flash:free",
            "openai/gpt-5.6-luna:batch",
            "google/gemini-3.8-flash-image",
        ] {
            let entry = entry_from(model_json(id, Some(CREATED_2026), STRUCTURED))?;
            assert!(!entry.known_good, "{id} should be excluded");
        }
        Ok(())
    }

    #[test]
    fn known_good_requires_structured_output_support() -> anyhow::Result<()> {
        let without = entry_from(model_json(
            "openai/gpt-5.6-luna",
            Some(CREATED_2026),
            &["tools"],
        ))?;
        let with_response_format = entry_from(model_json(
            "openai/gpt-5.6-luna",
            Some(CREATED_2026),
            &["response_format"],
        ))?;

        assert!(!without.known_good);
        assert!(!without.supports_structured_output);
        assert!(with_response_format.known_good);
        Ok(())
    }

    #[test]
    fn model_entry_carries_created_at() -> anyhow::Result<()> {
        let entry = entry_from(model_json("openai/gpt-5.6-luna", Some(1_780_000_123), &[]))?;

        assert_eq!(entry.created_at, Some(1_780_000_123));
        Ok(())
    }
}
