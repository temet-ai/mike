use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::runtime::{RerankStatus, Runtime};
use super::{
    HEALTH_TIMEOUT, MAX_DOCUMENT_CHARS, MAX_QUERY_CHARS, RerankConfig, RerankError, RerankScore,
    ServerHealth,
};
use crate::search_ranking::RankingConfig;

pub mod attempt;
pub mod score_cache;

use score_cache::ScoreCache;

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
    turn: tokio::sync::Mutex<()>,
    health_timeout: Duration,
    scores: ScoreCache,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reranked {
    pub scores: Vec<RerankScore>,
    pub cached: usize,
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

    pub fn with_health_timeout(config: &RerankConfig, health_timeout: Duration) -> Self {
        Self::assemble(config, health_timeout, ScoreCache::new())
    }

    fn assemble(config: &RerankConfig, health_timeout: Duration, scores: ScoreCache) -> Self {
        let mut config = config.clone();
        config.url = config.url.trim_end_matches('/').to_owned();
        Self {
            inner: Arc::new(Inner {
                config: RwLock::new(config),
                ranking: RwLock::new(RankingConfig::default()),
                client: reqwest::Client::new(),
                runtime: Mutex::new(Runtime::new()),
                turn: tokio::sync::Mutex::new(()),
                health_timeout,
                scores,
            }),
        }
    }

    #[cfg(test)]
    pub fn with_score_limits(
        config: &RerankConfig,
        capacity: usize,
        lifetime: Duration,
    ) -> Self {
        Self::assemble(
            config,
            HEALTH_TIMEOUT,
            ScoreCache::with_limits(capacity, lifetime),
        )
    }

    #[cfg(test)]
    pub fn age_cached_scores(&self, by: Duration) {
        self.inner.scores.advance(by);
    }

    #[cfg(test)]
    pub fn cached_score_count(&self) -> usize {
        self.inner.scores.len()
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

    pub fn status(&self) -> RerankStatus {
        let config = self.current_config();
        self.runtime()
            .status(config.enabled, config.url, config.model)
    }

    pub fn ranking(&self) -> RankingConfig {
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

    pub fn apply_stored_setting(&self, key: &str, value: &serde_json::Value) -> bool {
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
        super::assign_rerank_key(config, key, value)
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
        self.rerank_counted(query, documents, allow)
            .await
            .map(|reranked| reranked.scores)
    }

    pub async fn rerank_counted(
        &self,
        query: &str,
        documents: &[String],
        allow: impl Fn(&RerankConfig) -> bool,
    ) -> Result<Reranked, RerankError> {
        let config = self.current_config();
        if !config.enabled {
            return Err(RerankError::Disabled);
        }
        if !allow(&config) {
            return Err(RerankError::Refused(config.url.clone()));
        }
        if documents.is_empty() {
            return Ok(Reranked {
                scores: Vec::new(),
                cached: 0,
            });
        }
        let sent: Vec<&str> = documents
            .iter()
            .map(|document| truncate_chars(document, MAX_DOCUMENT_CHARS))
            .collect();
        let lookup =
            self.inner
                .scores
                .lookup(&config.model, truncate_chars(query, MAX_QUERY_CHARS), &sent);
        let missing = lookup.missing();
        let cached = lookup.cached();
        if missing.is_empty() {
            return Ok(Reranked {
                scores: lookup.merged(Vec::new(), &missing),
                cached,
            });
        }
        let subset: Vec<String> = missing
            .iter()
            .filter_map(|index| documents.get(*index).cloned())
            .collect();
        let asked = if cached == 0 { documents } else { &subset };
        let fresh = crate::metrics::time_phase(
            "rerank_request",
            self.request_scores(&config, query, asked),
        )
        .await?;
        self.inner.scores.store(&lookup, &missing, &fresh);
        Ok(Reranked {
            scores: lookup.merged(fresh, &missing),
            cached,
        })
    }

    async fn request_scores(
        &self,
        config: &RerankConfig,
        query: &str,
        documents: &[String],
    ) -> Result<Vec<RerankScore>, RerankError> {
        let overall = Instant::now() + config.call_budget();
        self.ensure_breaker_closed()?;
        let turn_wait = Duration::from_millis(config.cold_timeout_ms);
        let Ok(_turn) = tokio::time::timeout(turn_wait, self.inner.turn.lock()).await else {
            return Err(RerankError::Timeout(config.cold_timeout_ms));
        };
        self.ensure_breaker_closed()?;
        let cold = self.runtime().expects_cold_start();
        let outcome = self.attempts(config, query, documents, cold, overall).await;
        self.note(&outcome);
        outcome
    }

    fn ensure_breaker_closed(&self) -> Result<(), RerankError> {
        if self.runtime().breaker_open(Instant::now()) {
            return Err(RerankError::BreakerOpen);
        }
        Ok(())
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
}

fn held_open(error: &reqwest::Error) -> bool {
    error.is_timeout() && !error.is_connect()
}

pub fn truncate_chars(text: &str, max_chars: usize) -> &str {
    match text.char_indices().nth(max_chars) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

fn is_loading_body(body: &str) -> bool {
    body.to_ascii_lowercase().contains("loading model")
}

fn clip(body: &str) -> String {
    body.chars().take(ERROR_BODY_CHARS).collect()
}
