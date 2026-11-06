use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use super::observed::{
    BuildFacts, CapabilityFacts, ConfigFileFacts, ConnectionFacts, EmbeddingFacts, McpFacts,
    ModelFacts, Observed, ServiceFacts, ShellFacts, SidecarFacts, SpaFacts, ToolFacts,
};
use crate::claude_code::{Locations, gather_status, locate_mcp_binary};
use crate::config::ProxyConfig;
use crate::launch::cli::{
    ca_cert_candidates, locate_executable, resolve_ca_cert, sidecar_reachable,
};
use crate::tui::tcp_reachable;
use crate::user_config::{FileState, provenance};

const HTTP_TIMEOUT: Duration = Duration::from_secs(2);
const CLAUDE_VERSION_TIMEOUT: Duration = Duration::from_secs(5);
const KEY_VARIABLES: [&str; 2] = ["MIKE_STEERER_OPENROUTER_API_KEY", "OPENROUTER_API_KEY"];

pub(super) async fn observe(config: &ProxyConfig) -> Observed {
    let inspect_addr = connect_addr(&config.inspect_listen_addr);
    let proxy_addr = connect_addr(&config.proxy_listen_addr);
    let spa_addr = connect_addr(&config.spa_bind);
    let (database, proxy_up, inspect, spa_up, sidecar) = tokio::join!(
        super::observe_db::observe_database(config),
        tcp_reachable(&proxy_addr),
        InspectView::fetch(&inspect_addr),
        tcp_reachable(&spa_addr),
        observe_sidecar(config),
    );
    let embeddings = observe_embeddings(config, &inspect).await;
    let build = crate::change_log::current_build();
    let claude = observe_claude().await;
    let background = crate::service::observe().await;
    let (mcp, connection) = observe_claude_code();
    Observed {
        background,
        build: BuildFacts {
            line: crate::cli::version_line(),
            sha: build.sha.to_string(),
        },
        config_file: config_file_facts(),
        database,
        services: vec![
            ServiceFacts {
                name: "proxy",
                addr: config.proxy_listen_addr.clone(),
                enabled: true,
                answering: proxy_up,
                running_sha: None,
            },
            ServiceFacts {
                name: "inspect",
                addr: config.inspect_listen_addr.clone(),
                enabled: true,
                answering: inspect.health.is_some(),
                running_sha: inspect.build_sha(),
            },
            ServiceFacts {
                name: "spa",
                addr: config.spa_bind.clone(),
                enabled: config.spa_enabled,
                answering: spa_up,
                running_sha: None,
            },
        ],
        spa: SpaFacts {
            enabled: config.spa_enabled,
            embedded: crate::spa::embedded_in_binary(),
        },
        sidecar,
        models: observe_models(&claude, &inspect),
        claude,
        mcp,
        connection,
        embeddings,
        launch_backend: config.launch.backend.clone(),
    }
}

pub(crate) fn connect_addr(addr: &str) -> String {
    addr.replace("0.0.0.0", "127.0.0.1")
        .replace("[::]", "127.0.0.1")
}

fn config_file_facts() -> ConfigFileFacts {
    match provenance() {
        Some(p) => ConfigFileFacts {
            path: p.file.clone(),
            state: p.state.clone(),
        },
        None => ConfigFileFacts {
            path: None,
            state: FileState::NoLocation("the environment was not bootstrapped".to_string()),
        },
    }
}

pub(crate) struct InspectView {
    health: Option<Value>,
    capabilities: Option<Value>,
}

impl InspectView {
    pub(crate) async fn fetch(addr: &str) -> Self {
        let Ok(client) = reqwest::Client::builder().timeout(HTTP_TIMEOUT).build() else {
            return Self {
                health: None,
                capabilities: None,
            };
        };
        let (health, capabilities) = tokio::join!(
            get_json(&client, format!("http://{addr}/api/health")),
            get_json(&client, format!("http://{addr}/api/capabilities")),
        );
        Self {
            health,
            capabilities,
        }
    }

    pub(crate) fn build_sha(&self) -> Option<String> {
        self.health
            .as_ref()?
            .pointer("/build/sha")?
            .as_str()
            .map(str::to_string)
    }

    fn capability_rows(&self) -> Option<Vec<CapabilityFacts>> {
        let rows = self
            .capabilities
            .as_ref()?
            .get("capabilities")?
            .as_array()?;
        Some(
            rows.iter()
                .map(|row| CapabilityFacts {
                    id: text_field(row, "id"),
                    label: text_field(row, "label"),
                    state: text_field(row, "state"),
                })
                .collect(),
        )
    }
}

fn text_field(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

async fn get_json(client: &reqwest::Client, url: String) -> Option<Value> {
    let response = client.get(url).send().await.ok()?;
    response
        .status()
        .is_success()
        .then_some(response)?
        .json()
        .await
        .ok()
}

async fn observe_sidecar(config: &ProxyConfig) -> SidecarFacts {
    let cwd = std::env::current_dir().ok();
    let url = config.launch.proxy_forward_url.clone();
    SidecarFacts {
        reachable: sidecar_reachable(&url).await,
        ca_found: resolve_ca_cert(&config.launch, cwd.as_deref()),
        ca_candidates: ca_cert_candidates(&config.launch, cwd.as_deref()),
        url,
    }
}

async fn observe_claude() -> ToolFacts {
    let bin = crate::claude::claude_binary();
    let path = locate_executable(&bin, std::env::var_os("PATH").as_deref());
    let version_skipped = crate::sandbox::active();
    let version = match (&path, version_skipped) {
        (Some(path), false) => read_version(path).await,
        _ => None,
    };
    ToolFacts {
        path,
        version,
        version_skipped,
    }
}

async fn read_version(path: &Path) -> Option<String> {
    let run = tokio::process::Command::new(path)
        .arg("--version")
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(CLAUDE_VERSION_TIMEOUT, run)
        .await
        .ok()?
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let first = text.lines().next()?.trim();
    (output.status.success() && !first.is_empty()).then(|| first.to_string())
}

fn observe_claude_code() -> (McpFacts, ConnectionFacts) {
    let locations = match Locations::from_environment() {
        Ok(locations) => locations,
        Err(e) => {
            return (
                McpFacts {
                    binary: None,
                    registration: Err(e.to_string()),
                },
                ConnectionFacts {
                    route: None,
                    function_name: None,
                    shells: Vec::new(),
                },
            );
        }
    };
    let status = gather_status(&locations);
    let shells = status
        .shells
        .iter()
        .map(|s| ShellFacts {
            shell: s.shell.name(),
            block: s.block.clone(),
        })
        .collect();
    (
        McpFacts {
            binary: locate_mcp_binary(&locations),
            registration: status.registration,
        },
        ConnectionFacts {
            route: status.route,
            function_name: status.function_name,
            shells,
        },
    )
}

fn observe_models(claude: &ToolFacts, inspect: &InspectView) -> ModelFacts {
    let path_var = std::env::var_os("PATH");
    ModelFacts {
        claude_on_path: claude.path.is_some(),
        codex_on_path: locate_executable("codex", path_var.as_deref()).is_some(),
        openrouter_key_variable: KEY_VARIABLES
            .iter()
            .find(|name| std::env::var(name).is_ok_and(|v| !v.trim().is_empty()))
            .map(|name| (*name).to_string()),
        capabilities: inspect.capability_rows(),
    }
}

async fn observe_embeddings(config: &ProxyConfig, inspect: &InspectView) -> EmbeddingFacts {
    if let Some(rows) = inspect.capability_rows()
        && let Some(row) = rows.iter().find(|r| r.id == "semantic_recall")
    {
        let url = inspect
            .health
            .as_ref()
            .and_then(|h| h.pointer("/embedding/url"))
            .and_then(Value::as_str)
            .map(str::to_string);
        return match row.state.as_str() {
            "off" => EmbeddingFacts {
                url: None,
                reachable: None,
            },
            state => EmbeddingFacts {
                url,
                reachable: Some(state == "active"),
            },
        };
    }
    if !config.embedding.enabled {
        return EmbeddingFacts {
            url: None,
            reachable: None,
        };
    }
    let url = config.embedding.url.clone();
    let reachable = match crate::semantic::egress::url_host(&url) {
        Some(host) => {
            let port = reqwest::Url::parse(&url)
                .ok()
                .and_then(|u| u.port_or_known_default())
                .unwrap_or(80);
            Some(tcp_reachable(&format!("{host}:{port}")).await)
        }
        None => Some(false),
    };
    EmbeddingFacts {
        url: Some(url),
        reachable,
    }
}
