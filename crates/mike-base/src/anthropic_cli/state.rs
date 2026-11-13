use std::sync::Arc;

use super::cli::AnthropicCli;
use super::registry::LoginRegistry;
use super::status_cache::CliStatusCache;

#[derive(Clone)]
pub struct AnthropicCliState {
    pub cli: Arc<AnthropicCli>,
    pub status_cache: Arc<CliStatusCache>,
    pub logins: Arc<LoginRegistry>,
}

impl AnthropicCliState {
    pub fn new() -> Self {
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
