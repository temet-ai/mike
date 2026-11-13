//! How `mike spawn` makes sure the proxy stack is serving before it opens a
//! session. Starting the stack is the CLI's job (crate::stack); the binary
//! installs it here at start, so launch does not depend on the CLI layer.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::OnceLock;

use crate::config::ProxyConfig;

#[derive(Debug)]
pub enum EnsureOutcome {
    AlreadyRunning,
    Spawned { pid: u32, log_path: PathBuf },
}

pub trait StackStarter: Send + Sync {
    fn ensure_serving<'a>(
        &'a self,
        config: &'a ProxyConfig,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<EnsureOutcome>> + Send + 'a>>;

    fn probe_health<'a>(
        &'a self,
        inspect_addr: &'a str,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>>;
}

static STARTER: OnceLock<Box<dyn StackStarter>> = OnceLock::new();

/// Install the stack starter; the binary calls this once before `mike spawn`.
pub fn install(starter: Box<dyn StackStarter>) {
    if STARTER.set(starter).is_err() {
        tracing::debug!("a stack starter was already installed");
    }
}

pub fn starter() -> anyhow::Result<&'static dyn StackStarter> {
    STARTER
        .get()
        .map(|starter| starter.as_ref())
        .ok_or_else(|| anyhow::anyhow!("no stack starter is installed in this binary"))
}
