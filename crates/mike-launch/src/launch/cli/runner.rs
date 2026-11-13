use std::path::PathBuf;

use super::ledger::Ledger;
use crate::config::LaunchConfig;
use crate::launch::{ProxyEnv, TerminalBackend, TrustGate};

pub struct Runner<'a> {
    pub backend: &'a dyn TerminalBackend,
    pub ledger: Ledger,
    pub config: &'a LaunchConfig,
    pub proxy: Option<ProxyEnv>,
    pub trust: TrustGate,
    pub briefs_dir: PathBuf,
    pub inspect_addr: Option<String>,
    pub mcp_server: Option<PathBuf>,
    pub backend_reason: Option<String>,
}
