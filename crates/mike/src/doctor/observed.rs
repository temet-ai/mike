use std::path::PathBuf;

use crate::claude_code::{Registration, Route};
use crate::user_config::FileState;

#[derive(Debug, Clone)]
pub(crate) struct Observed {
    pub(crate) build: BuildFacts,
    pub(crate) config_file: ConfigFileFacts,
    pub(crate) database: DatabaseFacts,
    pub(crate) services: Vec<ServiceFacts>,
    pub(crate) background: Option<crate::service::BackgroundFacts>,
    pub(crate) spa: SpaFacts,
    pub(crate) sidecar: SidecarFacts,
    pub(crate) claude: ToolFacts,
    pub(crate) mcp: McpFacts,
    pub(crate) connection: ConnectionFacts,
    pub(crate) models: ModelFacts,
    pub(crate) embeddings: EmbeddingFacts,
    pub(crate) rerank: RerankFacts,
    pub(crate) launch_backend: String,
}

#[derive(Debug, Clone)]
pub(crate) struct BuildFacts {
    pub(crate) line: String,
    pub(crate) sha: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ConfigFileFacts {
    pub(crate) path: Option<PathBuf>,
    pub(crate) state: FileState,
}

#[derive(Debug, Clone)]
pub(crate) struct DatabaseFacts {
    pub(crate) persist_enabled: bool,
    pub(crate) redacted_url: Option<String>,
    pub(crate) server: Option<Result<ServerFacts, String>>,
}

#[derive(Debug, Clone)]
pub(crate) struct ServerFacts {
    pub(crate) version_num: i32,
    pub(crate) extensions: Vec<ExtensionFacts>,
    pub(crate) migrations: Result<MigrationFacts, String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ExtensionFacts {
    pub(crate) name: &'static str,
    pub(crate) installed: bool,
    pub(crate) available: bool,
    pub(crate) creatable: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct MigrationFacts {
    pub(crate) pending: usize,
    pub(crate) blocking: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ServiceFacts {
    pub(crate) name: &'static str,
    pub(crate) addr: String,
    pub(crate) enabled: bool,
    pub(crate) answering: bool,
    pub(crate) running_sha: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SpaFacts {
    pub(crate) enabled: bool,
    pub(crate) embedded: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct SidecarFacts {
    pub(crate) url: String,
    pub(crate) reachable: Result<u16, String>,
    pub(crate) ca_found: Option<PathBuf>,
    pub(crate) ca_candidates: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) struct ToolFacts {
    pub(crate) path: Option<PathBuf>,
    pub(crate) version: Option<String>,
    pub(crate) version_skipped: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct McpFacts {
    pub(crate) binary: Option<PathBuf>,
    pub(crate) registration: Result<Registration, String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ShellFacts {
    pub(crate) shell: &'static str,
    pub(crate) block: Result<bool, String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ConnectionFacts {
    pub(crate) route: Option<Route>,
    pub(crate) function_name: Option<String>,
    pub(crate) shells: Vec<ShellFacts>,
}

#[derive(Debug, Clone)]
pub(crate) struct ModelFacts {
    pub(crate) claude_on_path: bool,
    pub(crate) codex_on_path: bool,
    pub(crate) openrouter_key_variable: Option<String>,
    pub(crate) capabilities: Option<Vec<CapabilityFacts>>,
}

#[derive(Debug, Clone)]
pub(crate) struct CapabilityFacts {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) state: String,
}

#[derive(Debug, Clone)]
pub(crate) struct EmbeddingFacts {
    pub(crate) url: Option<String>,
    pub(crate) reachable: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RerankFacts {
    pub(crate) url: Option<String>,
    pub(crate) reachable: Option<bool>,
    pub(crate) detail: Option<String>,
}
