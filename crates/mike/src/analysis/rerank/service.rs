use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::runtime::{RerankStatus, Runtime};
use super::{
    HEALTH_TIMEOUT, MAX_DOCUMENT_CHARS, MAX_QUERY_CHARS, RerankConfig, RerankError, RerankScore,
    ServerHealth,
};
use crate::search_ranking::RankingConfig;

const ERROR_BODY_CHARS: usize = 500;
const RANKING_PREFIX: &str = "search.ranking.";

#[derive(Serialize)]
struct RerankRequest<'a> {
    model: &'a str,
    query: &'a str,
    documents: &'a [&'a str],
}

#[derive(Deserialize)]
struct RerankResponse {
    results: Vec<RerankResult>,
}

#[derive(Deserialize)]
struct RerankResult {
    index: usize,
    relevance_score: f32,
}

#[derive(Clone)]
pub struct RerankService {
    inner: Arc<Inner>,
}

struct Inner {
    config: RwLock<RerankConfig>,
    ranking: RwLock<RankingConfig>,
    client: reqwest::Client,
    runtime: Mutex<Runtime>,
    health_timeout: Duration,
}

impl std::fmt::Debug for RerankService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let config = self.current_config();
        f.debug_struct("RerankService")
            .field("enabled", &config.enabled)
            .field("url", &config.url)
            .field("model", &config.model)
            .finish()
    }
}

impl RerankService {
    pub fn new(config: &RerankConfig) -> Self {
        Self::with_health_timeout(config, HEALTH_TIMEOUT)
    }

    pub(super) fn with_health_timeout(config: &RerankConfig, health_timeout: Duration) -> Self {
        let mut config = config.clone();
        config.url = config.url.trim_end_matches('/').to_owned();
        Self {
            inner: Arc::new(Inner {
                config: RwLock::new(config),
                ranking: RwLock::new(RankingConfig::default()),
                client: reqwest::Client::new(),
                runtime: Mutex::new(Runtime::new()),
                health_timeout,
            }),
        }
    }

    pub fn current_config(&self) -> RerankConfig {
        self.inner
            .config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn is_enabled(&self) -> bool {
        self.current_config().enabled
    }

    pub(crate) fn status(&self) -> RerankStatus {
        let config = self.current_config();
        self.runtime()
            .status(config.enabled, config.url, config.model)
    }

    pub(crate) fn ranking(&self) -> RankingConfig {
        self.inner
            .ranking
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn apply_ranking(&self, key: &str, value: &serde_json::Value) -> bool {
        let mut ranking = self
            .inner
            .ranking
            .write()
            .unwrap_or_else(|e| e.into_inner());
        ranking.apply(key, value)
    }

    fn apply_stored_ranking(&self, key: &str, value: &serde_json::Value) -> bool {
        let mut ranking = self
            .inner
            .ranking
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let before = ranking.clone();
        ranking.apply(key, value) && *ranking != before
    }

    pub fn apply_setting(&self, key: &str, value: &serde_json::Value) -> bool {
        if key.starts_with(RANKING_PREFIX) {
            return self.apply_ranking(key, value);
        }
        let mut config = self.inner.config.write().unwrap_or_else(|e| e.into_inner());
        let applied = Self::assign_setting(&mut config, key, value);
        drop(config);
        if applied {
            self.reset_runtime_for(key);
        }
        applied
    }

    pub(crate) fn apply_stored_setting(&self, key: &str, value: &serde_json::Value) -> bool {
        if key.starts_with(RANKING_PREFIX) {
            return self.apply_stored_ranking(key, value);
        }
        let mut config = self.inner.config.write().unwrap_or_else(|e| e.into_inner());
        let before = config.clone();
        let changed = Self::assign_setting(&mut config, key, value) && *config != before;
        drop(config);
        if changed {
            self.reset_runtime_for(key);
        }
        changed
    }

    fn reset_runtime_for(&self, key: &str) {
        if matches!(key, "rerank.enabled" | "rerank.url" | "rerank.model") {
            *self.runtime() = Runtime::new();
        }
    }

    fn assign_setting(config: &mut RerankConfig, key: &str, value: &serde_json::Value) -> bool {
        match key {
            "rerank.enabled" => assign(&mut config.enabled, value.as_bool()),
            "rerank.url" => assign(
                &mut config.url,
                value.as_str().map(|v| v.trim_end_matches('/').to_owned()),
            ),
            "rerank.model" => assign(&mut config.model, value.as_str().map(str::to_owned)),
            "rerank.timeout_ms" => assign(&mut config.timeout_ms, value.as_u64().map(|v| v.max(1))),
            "rerank.candidates" => assign(
                &mut config.candidates,
                value
                    .as_u64()
                    .and_then(|v| usize::try_from(v).ok())
                    .map(|v| v.max(1)),
            ),
            "rerank.tuning.enabled" => assign(&mut config.tuning_enabled, value.as_bool()),
            _ => false,
        }
    }

    pub async fn health(&self) -> ServerHealth {
        let config = self.current_config();
        if !config.enabled {
            return ServerHealth::Off;
        }
        let cold = self.runtime().expects_cold_start();
        let outcome = self.get_health(&config, cold).await;
        let mut runtime = self.runtime();
        match &outcome {
            ServerHealth::Ready => runtime.record_ok(),
            ServerHealth::Loading => runtime.record_loading(),
            ServerHealth::Unavailable(reason) => runtime.record_failure(reason.clone()),
            ServerHealth::Off => {}
        }
        outcome
    }

    pub async fn rerank(
        &self,
        query: &str,
        documents: &[String],
        allow: impl Fn(&RerankConfig) -> bool,
    ) -> Result<Vec<RerankScore>, RerankError> {
        let config = self.current_config();
        if !config.enabled {
            return Err(RerankError::Disabled);
        }
        if !allow(&config) {
            return Err(RerankError::Refused(config.url.clone()));
        }
        if documents.is_empty() {
            return Ok(Vec::new());
        }
        if self.runtime().breaker_open(Instant::now()) {
            return Err(RerankError::BreakerOpen);
        }
        let cold = self.runtime().expects_cold_start();
        let outcome = match self.post_rerank(&config, query, documents).await {
            Err(RerankError::Timeout(_)) if cold => Err(RerankError::Loading),
            other => other,
        };
        self.note(&outcome);
        outcome
    }

    fn runtime(&self) -> std::sync::MutexGuard<'_, Runtime> {
        self.inner.runtime.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn note(&self, outcome: &Result<Vec<RerankScore>, RerankError>) {
        let mut runtime = self.runtime();
        match outcome {
            Ok(_) => runtime.record_ok(),
            Err(RerankError::Loading) => runtime.record_loading(),
            Err(
                error @ (RerankError::Timeout(_) | RerankError::Http(_) | RerankError::Parse(_)),
            ) => {
                runtime.record_failure(error.to_string());
            }
            Err(_) => {}
        }
    }

    async fn get_health(&self, config: &RerankConfig, cold: bool) -> ServerHealth {
        let url = format!("{}/health", config.url);
        let response = match self
            .inner
            .client
            .get(&url)
            .timeout(self.inner.health_timeout)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) if held_open(&error) && cold => return ServerHealth::Loading,
            Err(error) if error.is_timeout() => {
                return ServerHealth::Unavailable(format!(
                    "{url} did not answer within {} ms",
                    self.inner.health_timeout.as_millis()
                ));
            }
            Err(error) => return ServerHealth::Unavailable(format!("{url}: {error}")),
        };
        let status = response.status();
        if status.is_success() {
            return ServerHealth::Ready;
        }
        let body = response.text().await.unwrap_or_default();
        if status.as_u16() == 503 && is_loading_body(&body) {
            ServerHealth::Loading
        } else {
            ServerHealth::Unavailable(format!("{url} answered HTTP {status}: {}", clip(&body)))
        }
    }

    async fn post_rerank(
        &self,
        config: &RerankConfig,
        query: &str,
        documents: &[String],
    ) -> Result<Vec<RerankScore>, RerankError> {
        let url = format!("{}/v1/rerank", config.url);
        let bounded: Vec<&str> = documents
            .iter()
            .map(|d| truncate_chars(d, MAX_DOCUMENT_CHARS))
            .collect();
        let body = RerankRequest {
            model: &config.model,
            query: truncate_chars(query, MAX_QUERY_CHARS),
            documents: &bounded,
        };
        let response = self
            .inner
            .client
            .post(&url)
            .timeout(Duration::from_millis(config.timeout_ms))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if held_open(&e) {
                    RerankError::Timeout(config.timeout_ms)
                } else {
                    RerankError::Http(format!("{url}: {e}"))
                }
            })?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            if status.as_u16() == 503 && is_loading_body(&text) {
                return Err(RerankError::Loading);
            }
            return Err(RerankError::Http(format!(
                "{url} answered HTTP {status}: {}",
                clip(&text)
            )));
        }
        let parsed: RerankResponse = response.json().await.map_err(|e| {
            if e.is_timeout() {
                RerankError::Timeout(config.timeout_ms)
            } else {
                RerankError::Parse(e.to_string())
            }
        })?;
        into_scores(parsed, documents.len())
    }
}

fn held_open(error: &reqwest::Error) -> bool {
    error.is_timeout() && !error.is_connect()
}

pub(super) fn truncate_chars(text: &str, max_chars: usize) -> &str {
    match text.char_indices().nth(max_chars) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

fn assign<T>(slot: &mut T, value: Option<T>) -> bool {
    match value {
        Some(v) => {
            *slot = v;
            true
        }
        None => false,
    }
}

fn is_loading_body(body: &str) -> bool {
    body.to_ascii_lowercase().contains("loading model")
}

fn clip(body: &str) -> String {
    body.chars().take(ERROR_BODY_CHARS).collect()
}

fn into_scores(
    response: RerankResponse,
    document_count: usize,
) -> Result<Vec<RerankScore>, RerankError> {
    let mut scores = Vec::with_capacity(response.results.len());
    for result in response.results {
        if result.index >= document_count {
            return Err(RerankError::Parse(format!(
                "result index {} is outside the {document_count} documents sent",
                result.index
            )));
        }
        scores.push(RerankScore {
            index: result.index,
            score: result.relevance_score,
        });
    }
    scores.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.index.cmp(&b.index)));
    Ok(scores)
}
