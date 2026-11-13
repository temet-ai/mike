use serde::Serialize;

use crate::analysis::RerankError;
use crate::analysis::rerank::{RerankScore, Reranked};
use crate::semantic::KnowledgeState;
use crate::semantic::egress::allow_rerank;

#[cfg(test)]
pub mod rerank_search_tests;
pub mod short_query;
#[cfg(test)]
pub mod short_query_tests;

pub use short_query::short_query_report;

pub const COLD_START_ESTIMATE_SECS: u64 = 4;

const PLAIN_ORDER: &str = "these results are in plain order";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RerankState {
    Reranked,
    Loading,
    Off,
    Unavailable,
}

impl RerankState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reranked => "reranked",
            Self::Loading => "loading",
            Self::Off => "off",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RerankReport {
    pub state: RerankState,
    pub ready_in_s: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_out: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached: Option<usize>,
}

impl RerankReport {
    pub fn reranked() -> Self {
        Self {
            state: RerankState::Reranked,
            ready_in_s: Some(0),
            detail: None,
            left_out: Some(0),
            cached: None,
        }
    }

    pub fn with_cached(mut self, cached: usize) -> Self {
        self.cached = Some(cached);
        self
    }

    pub fn with_left_out(mut self, count: usize, line: f64) -> Self {
        self.left_out = Some(count);
        if count > 0 {
            let noun = if count == 1 {
                "candidate"
            } else {
                "candidates"
            };
            self.detail = Some(format!(
                "{count} {noun} scored below the relevance line {line} and {} left out",
                if count == 1 { "was" } else { "were" }
            ));
        }
        self
    }

    pub fn off(reason: &str) -> Self {
        Self {
            state: RerankState::Off,
            ready_in_s: None,
            detail: Some(format!("{reason}; {PLAIN_ORDER}")),
            left_out: None,
            cached: None,
        }
    }

    pub fn recent_order() -> Self {
        Self {
            state: RerankState::Off,
            ready_in_s: None,
            detail: Some(
                "the results are ordered by most recent activity, newest first; the reranker was not used"
                    .to_owned(),
            ),
            left_out: None,
            cached: None,
        }
    }

    pub fn switched_off() -> Self {
        Self::off("the reranker is switched off (rerank.enabled)")
    }

    pub fn from_error(error: &RerankError) -> Self {
        match error {
            RerankError::Disabled => Self::switched_off(),
            RerankError::Loading => Self {
                state: RerankState::Loading,
                ready_in_s: Some(COLD_START_ESTIMATE_SECS),
                detail: Some(format!(
                    "the reranker is loading, ready in about {COLD_START_ESTIMATE_SECS} s; {PLAIN_ORDER}"
                )),
                left_out: None,
                cached: None,
            },
            other => Self::unavailable(&unavailable_reason(other)),
        }
    }

    fn unavailable(reason: &str) -> Self {
        Self {
            state: RerankState::Unavailable,
            ready_in_s: None,
            detail: Some(format!("{reason}; {PLAIN_ORDER}")),
            left_out: None,
            cached: None,
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }
}

fn unavailable_reason(error: &RerankError) -> String {
    match error {
        RerankError::Refused(url) => format!(
            "the reranker at {url} is on another machine and semantic.allow_remote_embedding is off"
        ),
        RerankError::BreakerOpen => {
            "the reranker failed several times in a row and is paused for a short while".to_owned()
        }
        RerankError::Timeout(ms) => format!("the reranker did not answer within {ms} ms"),
        RerankError::Http(message) => format!("the reranker call failed ({message})"),
        RerankError::Parse(message) => format!("the reranker answered unreadably ({message})"),
        RerankError::Disabled | RerankError::Loading => "the reranker cannot be used".to_owned(),
    }
}

pub fn candidates_wanted(state: &KnowledgeState) -> Option<usize> {
    let config = state.rerank_service().current_config();
    config.enabled.then_some(config.candidates.max(1))
}

pub async fn score(
    state: &KnowledgeState,
    query: &str,
    documents: &[String],
) -> Result<Reranked, RerankReport> {
    let allow_remote = state.semantic().config().allow_remote_embedding;
    state
        .rerank_service()
        .rerank_counted(query, documents, |config| {
            allow_rerank(config, allow_remote)
        })
        .await
        .map_err(|error| RerankReport::from_error(&error))
}

pub fn arrange<T>(items: Vec<T>, scores: &[RerankScore]) -> Vec<(T, Option<f32>)> {
    let mut slots: Vec<Option<T>> = items.into_iter().map(Some).collect();
    let mut arranged = Vec::with_capacity(slots.len());
    for score in scores {
        if let Some(item) = slots.get_mut(score.index).and_then(Option::take) {
            arranged.push((item, Some(score.score)));
        }
    }
    arranged.extend(slots.into_iter().flatten().map(|item| (item, None)));
    arranged
}
