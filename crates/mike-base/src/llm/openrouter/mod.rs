use std::sync::Arc;
use std::time::Duration;

pub use call::CallUsage;
use call::is_retryable;
pub use key_guard::missing_key_error;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::time::sleep;

use super::budget::Budget;
use super::error::LlmError;
use super::limiter::{self, CallLimiter, LimiterSnapshot, Reservation};
use super::model_ref::{ModelRef, Provider};
use super::provider::{self, ModelTier};
use super::runtime::{LiveSettingsSource, LlmRuntimeSettings};
use super::usage::{LlmCallContext, LlmCallOutcome, LlmCallRecord, LlmRole, LlmUsageSink};
use super::{backoff, health, outage};

pub mod call;
pub mod claude_call;
pub mod cli_call;
pub mod codex_call;
pub mod jev_call;
pub mod key_guard;
pub mod recording;

#[cfg(test)]
pub mod attribution_tests;
#[cfg(test)]
pub mod basics_tests;
#[cfg(test)]
pub mod limiter_tests;
#[cfg(test)]
pub mod mandatory_reasoning_tests;
#[cfg(test)]
pub mod model_ref_tests;
#[cfg(test)]
pub mod provider_tests;
#[cfg(test)]
pub mod reasoning_tests;
#[cfg(test)]
pub mod recording_tests;
#[cfg(test)]
pub mod retry_tests;
#[cfg(test)]
pub mod test_support;

const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(8);

const MAX_ATTEMPTS: u32 = 3;

const RETRY_BACKOFF: [Duration; 2] = [Duration::from_millis(250), Duration::from_millis(500)];

#[derive(Clone)]
struct Inner {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
    timeout: Duration,
    limiter: Arc<CallLimiter>,
    runtime_settings: Option<LlmRuntimeSettings>,
    live_settings: Option<LiveSettingsSource>,
    usage_sink: Option<Arc<dyn LlmUsageSink>>,
    role: LlmRole,
    tier: Option<ModelTier>,
    call_context: LlmCallContext,
    reservation: Option<Reservation>,
    decision_store: Option<crate::store::persistence::PersistHandle>,
}

#[derive(Clone)]
pub struct OpenRouterClient {
    inner: Arc<Inner>,
}

impl OpenRouterClient {
    pub fn new(
        api_key: String,
        model: String,
        base_url: Option<String>,
        timeout: Option<Duration>,
    ) -> Result<Self, LlmError> {
        Self::new_with_limiter(api_key, model, base_url, timeout, limiter::global_limiter())
    }

    pub fn new_with_limiter(
        api_key: String,
        model: String,
        base_url: Option<String>,
        timeout: Option<Duration>,
        limiter: Arc<CallLimiter>,
    ) -> Result<Self, LlmError> {
        let http = reqwest::Client::builder()
            .build()
            .map_err(|e| LlmError::Transport(e.to_string()))?;

        Ok(Self {
            inner: Arc::new(Inner {
                http,
                api_key,
                model,
                base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
                timeout: timeout.unwrap_or(DEFAULT_TIMEOUT),
                limiter,
                runtime_settings: None,
                live_settings: None,
                usage_sink: None,
                role: LlmRole::default(),
                tier: None,
                call_context: LlmCallContext::default(),
                reservation: None,
                decision_store: None,
            }),
        })
    }

    pub fn with_model(&self, model: String) -> Result<Self, LlmError> {
        let mut inner = (*self.inner).clone();
        inner.model = model;
        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    pub fn model(&self) -> &str {
        &self.inner.model
    }

    pub fn with_api_key(&self, api_key: String) -> Result<Self, LlmError> {
        let mut inner = (*self.inner).clone();
        inner.api_key = api_key;
        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    pub fn with_runtime_settings(&self, settings: LlmRuntimeSettings) -> Self {
        let mut inner = (*self.inner).clone();
        inner.runtime_settings = Some(settings);
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn with_live_settings(&self, source: LiveSettingsSource) -> Self {
        let mut inner = (*self.inner).clone();
        inner.live_settings = Some(source);
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn effective_settings(&self) -> Option<LlmRuntimeSettings> {
        match &self.inner.live_settings {
            Some(source) => Some(source()),
            None => self.inner.runtime_settings.clone(),
        }
    }

    pub fn with_usage_sink(&self, sink: Arc<dyn LlmUsageSink>) -> Self {
        let mut inner = (*self.inner).clone();
        inner.usage_sink = Some(sink);
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn with_role(&self, role: LlmRole) -> Self {
        let mut inner = (*self.inner).clone();
        inner.role = role;
        inner.tier = ModelTier::for_role(role);
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn with_tier(&self, tier: ModelTier) -> Self {
        let mut inner = (*self.inner).clone();
        inner.tier = Some(tier);
        Self {
            inner: Arc::new(inner),
        }
    }

    fn provider_routed(&self) -> Self {
        let routed = provider::resolve(self.inner.tier, &self.inner.model);
        if routed == self.inner.model {
            return self.clone();
        }
        let mut inner = (*self.inner).clone();
        inner.model = routed;
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn with_attribution(
        &self,
        role: LlmRole,
        usage_sink: Option<&Arc<dyn LlmUsageSink>>,
    ) -> Self {
        let tagged = self.with_role(role);
        match usage_sink {
            Some(sink) => tagged.with_usage_sink(sink.clone()),
            None => tagged,
        }
    }

    pub fn with_call_context(&self, context: LlmCallContext) -> Self {
        let mut inner = (*self.inner).clone();
        inner.call_context = context;
        Self {
            inner: Arc::new(inner),
        }
    }

    #[cfg(test)]
    pub fn call_context(&self) -> LlmCallContext {
        self.inner.call_context
    }

    pub fn with_reservation(&self, reservation: &Reservation) -> Self {
        let mut inner = (*self.inner).clone();
        inner.reservation = Some(reservation.clone());
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn budget(&self) -> Budget {
        self.inner.limiter.available(self.inner.role)
    }

    pub fn reserve(&self, calls: u32) -> Reservation {
        self.inner.limiter.reserve(self.inner.role, calls)
    }

    pub fn limiter_snapshot(&self) -> LimiterSnapshot {
        self.inner.limiter.snapshot()
    }

    pub async fn verdict<T: DeserializeOwned>(
        &self,
        system: &str,
        user: &str,
        schema: Value,
    ) -> Result<T, LlmError> {
        self.verdict_with_usage(system, user, schema)
            .await
            .map(|(result, _)| result)
    }

    pub async fn verdict_with_usage<T: DeserializeOwned>(
        &self,
        system: &str,
        user: &str,
        schema: Value,
    ) -> Result<(T, CallUsage), LlmError> {
        let client = self.provider_routed();
        let transport = ModelRef::parse(&client.inner.model).provider;
        let role = client.inner.role;
        for attempt in 1..=MAX_ATTEMPTS {
            pause_for_timeouts(transport).await;
            let outcome = client.attempt_verdict(system, user, &schema).await;
            note_transport_outcome(transport, &outcome);
            match outcome {
                Ok(result) => {
                    health::record_success();
                    outage::note_success(role);
                    return Ok(result);
                }
                Err(e) => {
                    if !is_retryable(&e) || attempt == MAX_ATTEMPTS {
                        note_final_failure(role, &e);
                        return Err(e);
                    }
                    retry_delay(attempt, &e).await;
                }
            }
        }
        unreachable!("verdict loop must return within MAX_ATTEMPTS iterations")
    }
}

async fn pause_for_timeouts(transport: Provider) {
    let pause = backoff::pause(transport);
    if pause.is_zero() {
        return;
    }
    tracing::warn!(
        pause_secs = pause.as_secs(),
        "llm: backing off after consecutive timeouts on this transport"
    );
    sleep(pause).await;
}

fn note_transport_outcome<T>(transport: Provider, outcome: &Result<T, LlmError>) {
    if !matches!(outcome, Err(e) if e.is_refusal()) {
        backoff::record(transport, matches!(outcome, Err(LlmError::Timeout)));
    }
}

fn note_final_failure(role: LlmRole, error: &LlmError) {
    if error.is_refusal() {
        return;
    }
    health::record_failure(health::LlmFailureDetail::from(error));
    outage::note_failure(role, &error.to_string());
}

async fn retry_delay(attempt: u32, error: &LlmError) {
    let delay = RETRY_BACKOFF[(attempt - 1) as usize];
    tracing::warn!(
        attempt,
        max = MAX_ATTEMPTS,
        delay_ms = delay.as_millis(),
        error = %error,
        "openrouter verdict: retry {attempt}/{MAX_ATTEMPTS}"
    );
    sleep(delay).await;
}
