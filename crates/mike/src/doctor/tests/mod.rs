use std::path::PathBuf;

use serde_json::Value;

use super::checks::evaluate;
use super::code_scan::{BuiltinRules, CodeScanFacts};
use super::observed::{
    BuildFacts, CapabilityFacts, CodexFacts, ConfigFileFacts, ConnectionFacts, DatabaseFacts,
    EmbeddingFacts, ExtensionFacts, FeatureFacts, JobFacts, McpFacts, MigrationFacts, ModelFacts,
    Observed, PromptAssistantFacts, RerankFacts, ServerFacts, ServiceFacts, ShellFacts,
    SidecarFacts, SpaFacts, ToolFacts, WorkerProviderFacts,
};
use super::{Check, Report, SetupStep, Status, render};
use crate::claude_code::{Registration, Route};
use crate::user_config::FileState;

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn healthy() -> Observed {
    Observed {
        build: BuildFacts {
            line: "mike 0.1.0 (0123456, built now)".to_string(),
            sha: SHA.to_string(),
        },
        config_file: ConfigFileFacts {
            path: Some(PathBuf::from("/home/ada/.config/mike/config.toml")),
            state: FileState::Loaded { keys: 4 },
        },
        database: DatabaseFacts {
            persist_enabled: true,
            redacted_url: Some("postgres://mike:***@127.0.0.1:5432/mike".to_string()),
            server: Some(Ok(ServerFacts {
                version_num: 190_000,
                extensions: ["vector", "pgcrypto", "pg_trgm"]
                    .into_iter()
                    .map(|name| ExtensionFacts {
                        name,
                        installed: true,
                        available: true,
                        creatable: true,
                    })
                    .collect(),
                migrations: Ok(MigrationFacts {
                    pending: 0,
                    blocking: Vec::new(),
                }),
            })),
        },
        services: vec![
            service("proxy", "127.0.0.1:9988", None),
            service("inspect", "127.0.0.1:9989", Some(SHA)),
            service("spa", "127.0.0.1:9990", None),
        ],
        background: None,
        spa: SpaFacts {
            enabled: true,
            embedded: true,
        },
        sidecar: SidecarFacts {
            url: "http://127.0.0.1:9987".to_string(),
            reachable: Ok(400),
            ca_found: Some(PathBuf::from(
                "/home/ada/.local/share/proxy-forward/ca-cert.pem",
            )),
            ca_candidates: vec![PathBuf::from(
                "/home/ada/.local/share/proxy-forward/ca-cert.pem",
            )],
        },
        claude: ToolFacts {
            path: Some(PathBuf::from("/usr/local/bin/claude")),
            version: Some("2.1.0 (Claude Code)".to_string()),
            version_skipped: false,
        },
        mcp: McpFacts {
            binary: Some(PathBuf::from("/usr/local/bin/mike-mcp")),
            registration: Ok(Registration {
                command: Some("/usr/local/bin/mike-mcp".to_string()),
                command_exists: true,
                inspect_url: Some("http://127.0.0.1:9989".to_string()),
                in_a_project: false,
            }),
        },
        connection: ConnectionFacts {
            route: Some(Route::Sidecar),
            function_name: Some("cc".to_string()),
            shells: vec![ShellFacts {
                shell: "zsh",
                block: Ok(true),
            }],
        },
        models: ModelFacts {
            claude_on_path: true,
            codex_on_path: false,
            openrouter_key_variable: None,
            capabilities: Some(vec![CapabilityFacts {
                id: "persistence".to_string(),
                label: "Persistence".to_string(),
                state: "active".to_string(),
            }]),
        },
        embeddings: EmbeddingFacts {
            url: Some("http://127.0.0.1:7997".to_string()),
            reachable: Some(true),
        },
        rerank: RerankFacts::default(),
        launch_backend: "herdr".to_string(),
        code_scan: Some(healthy_code_scan()),
        features: healthy_features(),
    }
}

fn healthy_code_scan() -> CodeScanFacts {
    CodeScanFacts {
        scanning: true,
        delivering: false,
        pushing: false,
        grammars: vec!["bash".to_string(), "rust".to_string()],
        rules: Some(BuiltinRules {
            count: 42,
            version: "abc123".to_string(),
        }),
        scans_24h: 12,
        last_scan_at: Some("2026-11-14T09:00:00Z".to_string()),
    }
}

fn healthy_features() -> FeatureFacts {
    FeatureFacts {
        jobs: JobFacts {
            runnable: 4,
            missing: Vec::new(),
            needs_checkout: vec!["lessons_commit"],
        },
        worker_provider: WorkerProviderFacts {
            claude_on_service_path: Some(PathBuf::from("/home/ada/.local/bin/claude")),
            openrouter_key_variable: None,
        },
        codex: CodexFacts {
            on_path: false,
            home: Some(PathBuf::from("/home/ada/.codex")),
            hooks_installed: false,
        },
        prompt_assistant: PromptAssistantFacts {
            enabled: true,
            addr: "127.0.0.1:9985".to_string(),
            answering: true,
        },
    }
}

fn service(name: &'static str, addr: &str, sha: Option<&str>) -> ServiceFacts {
    ServiceFacts {
        name,
        addr: addr.to_string(),
        enabled: true,
        answering: true,
        running_sha: sha.map(str::to_string),
    }
}

fn check<'a>(checks: &'a [Check], id: &str) -> anyhow::Result<&'a Check> {
    checks.iter().find(|c| c.id == id).ok_or_else(|| {
        anyhow::anyhow!(
            "no check `{id}` in {:?}",
            checks.iter().map(|c| c.id).collect::<Vec<_>>()
        )
    })
}

fn server_mut(observed: &mut Observed) -> anyhow::Result<&mut ServerFacts> {
    match observed.database.server.as_mut() {
        Some(Ok(server)) => Ok(server),
        other => anyhow::bail!("fixture has no server facts: {other:?}"),
    }
}

mod background;
mod code_scan;
mod database;
mod features;
mod host;
mod report;
mod rerank;
