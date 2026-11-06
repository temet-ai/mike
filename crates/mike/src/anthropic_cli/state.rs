use std::sync::Arc;

use super::cli::AnthropicCli;
use super::registry::LoginRegistry;
use super::status_cache::CliStatusCache;

#[derive(Clone)]
pub(crate) struct AnthropicCliState {
    pub(crate) cli: Arc<AnthropicCli>,
    pub(crate) status_cache: Arc<CliStatusCache>,
    pub(crate) logins: Arc<LoginRegistry>,
}

impl AnthropicCliState {
    pub(crate) fn new() -> Self {
        Self {
            cli: Arc::new(AnthropicCli::with_binary(crate::llm::claude::binary())),
            status_cache: Arc::new(CliStatusCache::new()),
            logins: Arc::new(LoginRegistry::new()),
        }
    }
}

impl Default for AnthropicCliState {
    fn default() -> Self {
        Self::new()
    }
}
