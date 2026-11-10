use std::time::{Duration, Instant};

use super::{
    RerankRequest, RerankResponse, RerankService, clip, held_open, is_loading_body, truncate_chars,
};
use crate::analysis::rerank::{
    MAX_DOCUMENT_CHARS, MAX_QUERY_CHARS, RerankConfig, RerankError, RerankScore,
};

const LOADING_RETRY: Duration = Duration::from_millis(500);

impl RerankService {
    pub(super) async fn attempts(
        &self,
        config: &RerankConfig,
        query: &str,
        documents: &[String],
        cold: bool,
        overall: Instant,
    ) -> Result<Vec<RerankScore>, RerankError> {
        if cold {
            return match self.patient(config, query, documents, overall).await {
                Err(RerankError::Timeout(_)) => Err(RerankError::Loading),
                other => other,
            };
        }
        let first = Duration::from_millis(config.timeout_ms)
            .min(overall.saturating_duration_since(Instant::now()));
        match self.post_rerank(config, query, documents, first).await {
            Err(RerankError::Timeout(_) | RerankError::Loading) => {
                self.patient(config, query, documents, overall).await
            }
            other => other,
        }
    }

    async fn patient(
        &self,
        config: &RerankConfig,
        query: &str,
        documents: &[String],
        overall: Instant,
    ) -> Result<Vec<RerankScore>, RerankError> {
        let deadline =
            (Instant::now() + Duration::from_millis(config.cold_timeout_ms)).min(overall);
        let mut loading_seen = false;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(RerankError::Timeout(config.cold_timeout_ms));
            }
            let outcome = self.post_rerank(config, query, documents, remaining).await;
            match outcome {
                Err(RerankError::Loading) if Instant::now() + LOADING_RETRY < deadline => {
                    loading_seen = true;
                    tokio::time::sleep(LOADING_RETRY).await;
                }
                Err(RerankError::Timeout(_)) if loading_seen => return Err(RerankError::Loading),
                Err(RerankError::Timeout(_)) => {
                    return Err(RerankError::Timeout(config.cold_timeout_ms));
                }
                other => return other,
            }
        }
    }

    async fn post_rerank(
        &self,
        config: &RerankConfig,
        query: &str,
        documents: &[String],
        timeout: Duration,
    ) -> Result<Vec<RerankScore>, RerankError> {
        let url = format!("{}/v1/rerank", config.url);
        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
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
            .timeout(timeout)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if held_open(&e) {
                    RerankError::Timeout(timeout_ms)
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
                RerankError::Timeout(timeout_ms)
            } else {
                RerankError::Parse(e.to_string())
            }
        })?;
        into_scores(parsed, documents.len())
    }
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
