use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::{Semaphore, SemaphorePermit};

use super::model_ref::{ModelRef, Provider};
use super::provider::local::LOCAL_BUSY;
use super::{LlmError, OpenRouterClient};
use crate::circuit_breaker::CircuitBreaker;

#[cfg(test)]
#[path = "side_call_tests.rs"]
mod side_call_tests;

const BREAKER_THRESHOLD: u32 = 3;
const BREAKER_BASE_COOLDOWN: Duration = Duration::from_secs(60);
const BREAKER_MAX_COOLDOWN: Duration = Duration::from_secs(900);
const FALLBACK_WINDOW: Duration = Duration::from_secs(3600);
pub const DEFAULT_MAX_FALLBACK_CALLS_PER_HOUR: u32 = 20;
const USAGE_LIMIT_MARKERS: [&str; 3] = ["usage limit", "limit reached", "rate_limit"];

struct Link {
    client: OpenRouterClient,
    provider: Provider,
    breaker: Mutex<CircuitBreaker>,
    local_gate: Option<Semaphore>,
}

impl Link {
    fn new(client: OpenRouterClient) -> Self {
        let provider = ModelRef::parse(client.model()).provider;
        Self {
            client,
            provider,
            breaker: Mutex::new(CircuitBreaker::new(
                BREAKER_THRESHOLD,
                BREAKER_BASE_COOLDOWN,
                BREAKER_MAX_COOLDOWN,
            )),
            local_gate: (provider == Provider::Local).then(|| Semaphore::new(1)),
        }
    }

    fn is_local(&self) -> bool {
        self.provider == Provider::Local
    }

    fn skipped(&self) -> bool {
        self.breaker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .should_skip(Instant::now())
    }

    fn record(&self, succeeded: bool) {
        self.breaker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record(succeeded, Instant::now());
    }

    fn trip_for_max_cooldown(&self) {
        self.breaker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .trip_for_max_cooldown(Instant::now());
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SideCallOutcome<T> {
    pub value: T,
    pub model: String,
}

pub struct SideCallChain {
    links: Vec<Link>,
    max_fallback_calls_per_hour: u32,
    fallback_calls: Mutex<VecDeque<Instant>>,
}

struct Admission<'a> {
    _permit: Option<SemaphorePermit<'a>>,
}

fn falls_through(error: &LlmError) -> bool {
    match error {
        LlmError::Timeout
        | LlmError::Transport(_)
        | LlmError::Schema(_)
        | LlmError::OutputTruncated { .. }
        | LlmError::ProviderUnavailable { .. } => true,
        LlmError::Http { status } => matches!(status, 500..=599 | 401 | 403 | 404 | 429),
        LlmError::CeilingExceeded { .. }
        | LlmError::RateLimited { .. }
        | LlmError::SpendCeilingExceeded { .. } => false,
    }
}

fn is_local_busy(error: &LlmError) -> bool {
    matches!(error, LlmError::ProviderUnavailable { missing, .. } if missing == LOCAL_BUSY)
}

fn is_usage_limit(error: &LlmError) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    USAGE_LIMIT_MARKERS
        .iter()
        .any(|marker| text.contains(marker))
}

fn nothing_available(links: usize) -> LlmError {
    let missing = if links == 0 {
        "no model in the chain could be built".to_owned()
    } else {
        format!(
            "all {links} models in the chain are cooling down, busy or over the hourly fallback cap"
        )
    };
    LlmError::ProviderUnavailable {
        model: "side-call chain".to_owned(),
        provider: "side call",
        missing,
    }
}

impl SideCallChain {
    pub fn new(clients: Vec<OpenRouterClient>) -> Self {
        Self {
            links: clients.into_iter().map(Link::new).collect(),
            max_fallback_calls_per_hour: DEFAULT_MAX_FALLBACK_CALLS_PER_HOUR,
            fallback_calls: Mutex::new(VecDeque::new()),
        }
    }

    pub fn with_max_fallback_calls_per_hour(mut self, max: u32) -> Self {
        self.max_fallback_calls_per_hour = max;
        self
    }

    pub fn from_models(
        models: &[String],
        build: impl Fn(&str) -> Option<OpenRouterClient>,
    ) -> Self {
        Self::new(models.iter().filter_map(|model| build(model)).collect())
    }

    pub fn len(&self) -> usize {
        self.links.len()
    }

    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }

    fn within_fallback_cap(&self) -> bool {
        let now = Instant::now();
        let mut calls = self
            .fallback_calls
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        while calls
            .front()
            .is_some_and(|at| now.duration_since(*at) >= FALLBACK_WINDOW)
        {
            calls.pop_front();
        }
        if calls.len() >= self.max_fallback_calls_per_hour as usize {
            return false;
        }
        calls.push_back(now);
        true
    }

    fn admit<'a>(&'a self, link: &'a Link) -> Option<Admission<'a>> {
        if link.skipped() {
            return None;
        }
        if let Some(gate) = &link.local_gate {
            return gate.try_acquire().ok().map(|permit| Admission {
                _permit: Some(permit),
            });
        }
        self.within_fallback_cap()
            .then_some(Admission { _permit: None })
    }

    fn note_failure(link: &Link, error: &LlmError) {
        if is_local_busy(error) {
            return;
        }
        if link.provider == Provider::Claude && is_usage_limit(error) {
            link.trip_for_max_cooldown();
            return;
        }
        link.record(false);
    }

    pub async fn verdict<T: DeserializeOwned>(
        &self,
        system: &str,
        user: &str,
        schema: Value,
    ) -> Result<SideCallOutcome<T>, LlmError> {
        let mut last_error = None;
        for link in &self.links {
            let Some(_admission) = self.admit(link) else {
                continue;
            };
            match link
                .client
                .attempt_verdict::<T>(system, user, &schema)
                .await
            {
                Ok((value, _usage)) => {
                    link.record(true);
                    return Ok(SideCallOutcome {
                        value,
                        model: link.client.model().to_owned(),
                    });
                }
                Err(error) if falls_through(&error) => {
                    Self::note_failure(link, &error);
                    tracing::warn!(
                        model = link.client.model(),
                        local = link.is_local(),
                        error = %error,
                        "side call: model failed, trying the next one"
                    );
                    last_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }
        Err(last_error.unwrap_or_else(|| nothing_available(self.links.len())))
    }
}
