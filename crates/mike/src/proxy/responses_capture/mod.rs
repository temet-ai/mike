mod correlation;
mod headers;
mod record;
mod relay;
mod tee;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::any;
use tokio::sync::mpsc;

use crate::config::AppState;
use crate::store::persistence::PersistHandle;

pub(crate) use correlation::{Correlation, correlate};

pub(crate) const ROUTE_PREFIX: &str = "/codex";
pub(crate) const OPENAI_UPSTREAM_ENV: &str = "MIKE_RESPONSES_OPENAI_UPSTREAM";
pub(crate) const CHATGPT_UPSTREAM_ENV: &str = "MIKE_RESPONSES_CHATGPT_UPSTREAM";
pub(crate) const DEFAULT_OPENAI_UPSTREAM: &str = "https://api.openai.com/v1";
pub(crate) const DEFAULT_CHATGPT_UPSTREAM: &str = "https://chatgpt.com/backend-api/codex";
const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthRoute {
    Chatgpt,
    ApiKey,
    None,
}

impl AuthRoute {
    pub(crate) fn as_db_str(self) -> &'static str {
        match self {
            AuthRoute::Chatgpt => "chatgpt",
            AuthRoute::ApiKey => "api_key",
            AuthRoute::None => "none",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Upstreams {
    pub(crate) openai: String,
    pub(crate) chatgpt: String,
}

impl Upstreams {
    pub(crate) fn from_env() -> Self {
        Self::from_lookup(&|key| std::env::var(key).ok())
    }

    pub(crate) fn from_lookup(lookup: &dyn Fn(&str) -> Option<String>) -> Self {
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

    pub(crate) fn base_for(&self, route: AuthRoute) -> &str {
        match route {
            AuthRoute::Chatgpt => &self.chatgpt,
            AuthRoute::ApiKey | AuthRoute::None => &self.openai,
        }
    }
}

#[derive(Clone)]
pub(crate) struct ResponsesState {
    pub(crate) client: reqwest::Client,
    pub(crate) upstreams: Arc<Upstreams>,
    pub(crate) persist: Option<PersistHandle>,
    pub(crate) registered: Arc<Mutex<HashMap<uuid::Uuid, Instant>>>,
    pub(crate) recorded: Option<mpsc::UnboundedSender<Result<i64, String>>>,
}

impl ResponsesState {
    pub(crate) fn new(
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
        }
    }
}

pub(crate) fn router(state: &AppState) -> Router<AppState> {
    let responses = ResponsesState::new(
        state.client.clone(),
        Upstreams::from_env(),
        state.persist().cloned(),
    );
    routes_with(responses)
}

#[cfg(test)]
pub(crate) fn routes(state: ResponsesState) -> Router {
    routes_with(state)
}

fn routes_with<S: Clone + Send + Sync + 'static>(state: ResponsesState) -> Router<S> {
    Router::new()
        .route(&format!("{ROUTE_PREFIX}/{{*rest}}"), any(relay::handle))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .with_state(state)
}

#[cfg(test)]
#[path = "relay_tests.rs"]
mod relay_tests;
