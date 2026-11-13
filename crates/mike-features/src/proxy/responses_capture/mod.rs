pub mod correlation;
pub mod diagnostic;
pub mod headers;
pub mod record;
pub mod relay;
pub mod request_body;
pub mod tee;
pub mod wire_observation;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::Router;
use axum::routing::any;
use tokio::sync::{Semaphore, mpsc};

use crate::state::AppState;
use crate::store::persistence::PersistHandle;

pub use correlation::{Correlation, correlate};

pub const ROUTE_PREFIX: &str = "/codex";
pub const OPENAI_UPSTREAM_ENV: &str = "MIKE_RESPONSES_OPENAI_UPSTREAM";
pub const CHATGPT_UPSTREAM_ENV: &str = "MIKE_RESPONSES_CHATGPT_UPSTREAM";
pub const DEFAULT_OPENAI_UPSTREAM: &str = "https://api.openai.com/v1";
pub const DEFAULT_CHATGPT_UPSTREAM: &str = "https://chatgpt.com/backend-api/codex";
pub const MAX_DIAGNOSTIC_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthRoute {
    Chatgpt,
    ApiKey,
    None,
}

impl AuthRoute {
    pub fn as_db_str(self) -> &'static str {
        match self {
            AuthRoute::Chatgpt => "chatgpt",
            AuthRoute::ApiKey => "api_key",
            AuthRoute::None => "none",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upstreams {
    pub openai: String,
    pub chatgpt: String,
}

impl Upstreams {
    pub fn from_env() -> Self {
        Self::from_lookup(&|key| std::env::var(key).ok())
    }

    pub fn from_lookup(lookup: &dyn Fn(&str) -> Option<String>) -> Self {
        let pick = |key: &str, default: &str| {
            lookup(key)
                .map(|v| v.trim().trim_end_matches('/').to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| default.to_string())
        };
        Self {
            openai: pick(OPENAI_UPSTREAM_ENV, DEFAULT_OPENAI_UPSTREAM),
            chatgpt: pick(CHATGPT_UPSTREAM_ENV, DEFAULT_CHATGPT_UPSTREAM),
        }
    }

    pub fn base_for(&self, route: AuthRoute) -> &str {
        match route {
            AuthRoute::Chatgpt => &self.chatgpt,
            AuthRoute::ApiKey | AuthRoute::None => &self.openai,
        }
    }
}

#[derive(Clone)]
pub struct ResponsesState {
    pub client: reqwest::Client,
    pub upstreams: Arc<Upstreams>,
    pub persist: Option<PersistHandle>,
    pub registered: Arc<Mutex<HashMap<uuid::Uuid, Instant>>>,
    pub recorded: Option<mpsc::UnboundedSender<Result<i64, String>>>,
    pub slots: Arc<Semaphore>,
    pub toggles: Arc<arc_swap::ArcSwap<crate::feature_toggles::FeatureToggles>>,
}

impl ResponsesState {
    pub fn new(
        client: reqwest::Client,
        upstreams: Upstreams,
        persist: Option<PersistHandle>,
    ) -> Self {
        Self {
            client,
            upstreams: Arc::new(upstreams),
            persist,
            registered: Arc::new(Mutex::new(HashMap::new())),
            recorded: None,
            slots: Arc::new(Semaphore::new(16)),
            toggles: Arc::new(arc_swap::ArcSwap::from_pointee(Default::default())),
        }
    }
}

pub fn router(state: &AppState) -> Router<AppState> {
    let mut responses = ResponsesState::new(
        state.client.clone(),
        Upstreams::from_env(),
        state.persist().cloned(),
    );
    responses.toggles = state.feature_toggle_overlay.clone();
    routes_with(responses)
}

#[cfg(test)]
pub fn routes(state: ResponsesState) -> Router {
    routes_with(state)
}

fn routes_with<S: Clone + Send + Sync + 'static>(state: ResponsesState) -> Router<S> {
    Router::new()
        .route(&format!("{ROUTE_PREFIX}/{{*rest}}"), any(relay::handle))
        .with_state(state)
}

#[cfg(test)]
#[path = "relay_tests.rs"]
pub mod relay_tests;
