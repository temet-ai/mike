use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
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
pub const DEFAULT_ESCALATE_AFTER: Duration = Duration::from_secs(15);
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

    fn trip(&self) {
        self.breaker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .trip(Instant::now());
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
    escalate_after: Duration,
}

struct Ask<'a> {
    system: &'a str,
    user: &'a str,
    schema: &'a Value,
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
        | LlmError::ProviderUnavailable { .. }
        | LlmError::CeilingExceeded { .. }
        | LlmError::RateLimited { .. }
        | LlmError::SpendCeilingExceeded { .. } => true,
        LlmError::Http { status } => matches!(status, 500..=599 | 401 | 403 | 404 | 429),
    }
}

fn is_limiter_refusal(error: &LlmError) -> bool {
    matches!(
        error,
        LlmError::CeilingExceeded { .. }
            | LlmError::RateLimited { .. }
            | LlmError::SpendCeilingExceeded { .. }
    )
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
            escalate_after: DEFAULT_ESCALATE_AFTER,
        }
    }

    pub fn with_max_fallback_calls_per_hour(mut self, max: u32) -> Self {
        self.max_fallback_calls_per_hour = max;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_escalate_after(mut self, after: Duration) -> Self {
        self.escalate_after = after;
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
        if is_local_busy(error) || is_limiter_refusal(error) {
            return;
        }
        if link.provider == Provider::Claude && is_usage_limit(error) {
            link.trip_for_max_cooldown();
            return;
        }
        if link.is_local() && matches!(error, LlmError::Timeout) {
            link.trip();
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
        let ask = Ask {
            system,
            user,
            schema: &schema,
        };
        match self.links.first().filter(|link| link.is_local()) {
            Some(local) => self.local_first(local, &ask).await,
            None => self.in_order(0, &ask).await,
        }
    }

    async fn local_first<T: DeserializeOwned>(
        &self,
        local: &Link,
        ask: &Ask<'_>,
    ) -> Result<SideCallOutcome<T>, LlmError> {
        let mut attempt = std::pin::pin!(self.attempt::<T>(local, ask));
        match tokio::time::timeout(self.escalate_after, &mut attempt).await {
            Ok(settled) => self.after_local(settled, ask).await,
            Err(_) => self.race(local, attempt, ask).await,
        }
    }

    async fn after_local<T: DeserializeOwned>(
        &self,
        settled: Option<Result<SideCallOutcome<T>, LlmError>>,
        ask: &Ask<'_>,
    ) -> Result<SideCallOutcome<T>, LlmError> {
        match settled {
            Some(Ok(outcome)) => Ok(outcome),
            Some(Err(error)) if !falls_through(&error) => Err(error),
            _ => self.in_order(1, ask).await,
        }
    }

    async fn race<T: DeserializeOwned, F>(
        &self,
        local: &Link,
        mut attempt: Pin<&mut F>,
        ask: &Ask<'_>,
    ) -> Result<SideCallOutcome<T>, LlmError>
    where
        F: Future<Output = Option<Result<SideCallOutcome<T>, LlmError>>>,
    {
        tracing::info!(
            model = local.client.model(),
            after_secs = self.escalate_after.as_secs(),
            "side call: local model is slow, asking the next one too"
        );
        let mut escalated = std::pin::pin!(self.in_order::<T>(1, ask));
        tokio::select! {
            settled = &mut attempt => match settled {
                Some(Ok(outcome)) => Ok(outcome),
                _ => escalated.await,
            },
            answered = &mut escalated => Self::fallback_answered(local, answered, attempt).await,
        }
    }

    async fn fallback_answered<T, F>(
        local: &Link,
        answered: Result<SideCallOutcome<T>, LlmError>,
        attempt: Pin<&mut F>,
    ) -> Result<SideCallOutcome<T>, LlmError>
    where
        F: Future<Output = Option<Result<SideCallOutcome<T>, LlmError>>>,
    {
        if answered.is_ok() {
            Self::note_failure(local, &LlmError::Timeout);
            return answered;
        }
        match attempt.await {
            Some(Ok(outcome)) => Ok(outcome),
            _ => answered,
        }
    }

    async fn in_order<T: DeserializeOwned>(
        &self,
        start: usize,
        ask: &Ask<'_>,
    ) -> Result<SideCallOutcome<T>, LlmError> {
        let mut last_error = None;
        for link in self.links.iter().skip(start) {
            match self.attempt::<T>(link, ask).await {
                None => {}
                Some(Ok(outcome)) => return Ok(outcome),
                Some(Err(error)) if falls_through(&error) => last_error = Some(error),
                Some(Err(error)) => return Err(error),
            }
        }
        Err(last_error.unwrap_or_else(|| nothing_available(self.links.len())))
    }

    async fn attempt<T: DeserializeOwned>(
        &self,
        link: &Link,
        ask: &Ask<'_>,
    ) -> Option<Result<SideCallOutcome<T>, LlmError>> {
        let _admission = self.admit(link)?;
        let result = link
            .client
            .attempt_verdict::<T>(ask.system, ask.user, ask.schema)
            .await;
        Some(match result {
            Ok((value, _usage)) => {
                link.record(true);
                Ok(SideCallOutcome {
                    value,
                    model: link.client.model().to_owned(),
                })
            }
            Err(error) => {
                if falls_through(&error) {
                    Self::note_failure(link, &error);
                    tracing::warn!(
                        model = link.client.model(),
                        local = link.is_local(),
                        error = %error,
                        "side call: model failed, trying the next one"
                    );
                }
                Err(error)
            }
        })
    }
}
