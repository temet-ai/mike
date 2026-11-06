use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use super::{
    DEFAULT_DIM, EmbedError, EmbeddingConfig, HTTP_TIMEOUT, PREFIX_DOCUMENT, PREFIX_QUERY,
};

#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(Deserialize)]
struct EmbedResponse {
    data: Vec<EmbedData>,
}

#[derive(Deserialize)]
struct EmbedData {
    embedding: Vec<f32>,
}

#[derive(Clone)]
pub struct EmbeddingService {
    inner: Arc<Inner>,
}

struct Inner {
    config: RwLock<EmbeddingConfig>,
    client: reqwest::Client,
    attributed_rss_bytes: AtomicU64,
}

impl std::fmt::Debug for EmbeddingService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let config = self.snapshot_config();
        f.debug_struct("EmbeddingService")
            .field("enabled", &config.enabled)
            .field("url", &config.url)
            .field("model", &config.model)
            .finish()
    }
}

impl EmbeddingService {
    pub fn new(config: &EmbeddingConfig) -> Self {
        let client = reqwest::Client::new();

        Self {
            inner: Arc::new(Inner {
                config: RwLock::new(config.clone()),
                client,
                attributed_rss_bytes: AtomicU64::new(0),
            }),
        }
    }

    fn snapshot_config(&self) -> EmbeddingConfig {
        self.inner
            .config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn apply_setting(&self, key: &str, value: &serde_json::Value) -> bool {
        let mut config = self.inner.config.write().unwrap_or_else(|e| e.into_inner());
        match key {
            "embedding.enabled" => {
                if let Some(v) = value.as_bool() {
                    config.enabled = v;
                    true
                } else {
                    false
                }
            }
            "embedding.url" => {
                if let Some(v) = value.as_str() {
                    config.url = v.trim_end_matches('/').to_string();
                    true
                } else {
                    false
                }
            }
            "embedding.model" => {
                if let Some(v) = value.as_str() {
                    config.model = v.to_owned();
                    true
                } else {
                    false
                }
            }
            "embedding.chunk_max_chars" => {
                if let Some(v) = value.as_u64() {
                    config.chunk_max_chars = v.max(1) as usize;
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    pub fn current_config(&self) -> EmbeddingConfig {
        self.snapshot_config()
    }

    pub fn is_available(&self) -> bool {
        self.snapshot_config().enabled
    }

    pub fn model_name(&self) -> String {
        self.snapshot_config().model
    }

    pub fn url(&self) -> String {
        self.snapshot_config().url
    }

    pub fn dim(&self) -> usize {
        DEFAULT_DIM
    }

    pub fn chunk_max_chars(&self) -> usize {
        self.snapshot_config().chunk_max_chars
    }

    pub fn attributed_rss_bytes(&self) -> u64 {
        self.inner.attributed_rss_bytes.load(Ordering::Acquire)
    }

    pub async fn warm(&self) -> Result<(), EmbedError> {
        Ok(())
    }

    pub async fn embed_query(&self, query: &str) -> Result<Vec<f32>, EmbedError> {
        let prefixed = format!("{PREFIX_QUERY}{query}");
        let mut out = self.embed_batch(vec![prefixed]).await?;
        out.pop()
            .ok_or_else(|| EmbedError::Parse("empty response for single query".to_string()))
    }

    pub async fn embed_documents(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let prefixed: Vec<String> = texts
            .iter()
            .map(|t| format!("{PREFIX_DOCUMENT}{t}"))
            .collect();
        self.embed_batch(prefixed).await
    }

    pub async fn embed_documents_checked(
        &self,
        texts: &[String],
        allow: impl Fn(&EmbeddingConfig) -> bool,
    ) -> Result<(EmbeddingConfig, Vec<Vec<f32>>), EmbedError> {
        let config = self.snapshot_config();
        if !config.enabled {
            return Err(EmbedError::Disabled);
        }
        if !allow(&config) {
            return Err(EmbedError::Refused(config.url.clone()));
        }
        if texts.is_empty() {
            return Ok((config, Vec::new()));
        }
        let prefixed: Vec<String> = texts
            .iter()
            .map(|t| format!("{PREFIX_DOCUMENT}{t}"))
            .collect();
        let vectors = self.post_to_infinity(&config, &prefixed).await?;
        Ok((config, vectors))
    }

    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        let mut out = self.embed_batch(vec![text.to_string()]).await?;
        out.pop()
            .ok_or_else(|| EmbedError::Parse("empty response for single embed".to_string()))
    }

    pub async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EmbedError> {
        let config = self.snapshot_config();
        if !config.enabled {
            return Err(EmbedError::Disabled);
        }
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        self.post_to_infinity(&config, &texts).await
    }

    async fn post_to_infinity(
        &self,
        config: &EmbeddingConfig,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, EmbedError> {
        let url = format!("{}/embeddings", config.url.trim_end_matches('/'));
        let body = EmbedRequest {
            model: &config.model,
            input: texts,
        };

        let resp = self
            .inner
            .client
            .post(&url)
            .timeout(HTTP_TIMEOUT)
            .json(&body)
            .send()
            .await
            .map_err(|e| EmbedError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(EmbedError::Http(format!("HTTP {status}: {body_text}")));
        }

        let parsed: EmbedResponse = resp
            .json()
            .await
            .map_err(|e| EmbedError::Parse(e.to_string()))?;

        let expected = texts.len();
        if parsed.data.len() != expected {
            return Err(EmbedError::LengthMismatch {
                got: parsed.data.len(),
                expected,
            });
        }

        Ok(parsed.data.into_iter().map(|d| d.embedding).collect())
    }
}
