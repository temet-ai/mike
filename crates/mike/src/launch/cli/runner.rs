use std::path::PathBuf;

use super::ledger::Ledger;
use crate::config::LaunchConfig;
use crate::launch::{ProxyEnv, TerminalBackend, TrustGate};

pub(crate) struct Runner<'a> {
    pub(crate) backend: &'a dyn TerminalBackend,
    pub(crate) ledger: Ledger,
    pub(crate) config: &'a LaunchConfig,
    pub(crate) proxy: Option<ProxyEnv>,
    pub(crate) trust: TrustGate,
    pub(crate) briefs_dir: PathBuf,
    pub(crate) inspect_addr: Option<String>,
    pub(crate) mcp_server: Option<PathBuf>,
    pub(crate) backend_reason: Option<String>,
}
