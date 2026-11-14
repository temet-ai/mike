use std::time::Duration;

use crate::config::ProxyConfig;
use crate::store::persistence::{PersistError, migrations, pool};

use super::installation::Check;

fn available(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|path| path.join(name).is_file()))
}

pub(super) fn local_tools() -> Vec<Check> {
    let mcp = crate::setup::mcp::companion_binary().is_ok();
    vec![
        if mcp {
            Check::new(
                "mcp_binary",
                true,
                "pass",
                "MCP_BINARY_FOUND",
                "MCP companion exists; run the installed stdio handshake to qualify the connection.",
            )
        } else {
            Check::new(
                "mcp_binary",
                true,
                "fail",
                "MCP_BINARY_MISSING",
                "Install an executable matching mike-mcp companion through the stable installation prefix or on PATH.",
            )
        },
        if available("claude") {
            Check::new(
                "claude",
                false,
                "pass",
                "CLAUDE_FOUND",
                "Claude client exists; real capture compatibility is a separate check.",
            )
        } else {
            Check::new(
                "claude",
                false,
                "unavailable",
                "CLAUDE_ABSENT",
                "Optional Claude client is not installed; standalone proxy and inspection remain usable.",
            )
        },
        if available("docker") {
            Check::new(
                "docker",
                false,
                "pass",
                "DOCKER_CLIENT_FOUND",
                "Docker client exists; daemon and Compose readiness are checked by managed setup.",
            )
        } else {
            Check::new(
                "docker",
                false,
                "unavailable",
                "DOCKER_ABSENT",
                "Docker is optional for an existing PostgreSQL database or trial mode.",
            )
        },
    ]
}

pub(super) async fn database(config: &ProxyConfig) -> Check {
    if !config.persist_enabled {
        return Check::new(
            "persistence",
            false,
            "pass",
            "TRIAL_MODE",
            "Trial history is ephemeral; database readiness is not required.",
        );
    }
    let Some(url) = config.database_url.as_deref() else {
        return Check::new(
            "persistence",
            true,
            "fail",
            "DATABASE_NOT_CONFIGURED",
            "Persistent mode requires a selected database. Configure one or explicitly choose trial mode.",
        );
    };
    let outcome = tokio::time::timeout(Duration::from_secs(5), async {
        let connection = pool::build_read_only_pool(url).await?;
        let assessment = assess_database(&connection).await;
        connection.close().await;
        assessment
    })
    .await;
    match outcome {
        Ok(Ok(DatabaseReadiness::Current)) => Check::new(
            "persistence",
            true,
            "pass",
            "DATABASE_CURRENT",
            "Database prerequisites, schema and migration ledger match this binary; no migration was run.",
        ),
        Ok(Ok(DatabaseReadiness::SchemaNotCurrent))
        | Ok(Err(PersistError::SchemaNotCurrent(_))) => Check::new(
            "persistence",
            true,
            "fail",
            "DATABASE_SCHEMA_NOT_CURRENT",
            "Database schema or migration ledger differs from this binary. Review migrate-check before starting the owned service.",
        ),
        Ok(Ok(DatabaseReadiness::PrerequisitesFailed)) => Check::new(
            "persistence",
            true,
            "fail",
            "DATABASE_PREREQUISITES_FAILED",
            "Database prerequisites failed. Check PostgreSQL 19, the writable primary, role ownership, public search_path and required extensions; review setup validation.",
        ),
        Ok(Err(_)) => Check::new(
            "persistence",
            true,
            "fail",
            "DATABASE_UNAVAILABLE",
            "Read-only database check failed. Check connectivity, credentials and permissions; connection details are not printed.",
        ),
        Err(_) => Check::new(
            "persistence",
            true,
            "fail",
            "DATABASE_TIMEOUT",
            "Read-only database check exceeded five seconds. No migration was attempted.",
        ),
    }
}

enum DatabaseReadiness {
    Current,
    SchemaNotCurrent,
    PrerequisitesFailed,
}

async fn assess_database(connection: &sqlx::PgPool) -> Result<DatabaseReadiness, PersistError> {
    let ledger = migrations::assess(connection).await?;
    if ledger.not_current_reason().is_some() || ledger.is_blocking() {
        return Ok(DatabaseReadiness::SchemaNotCurrent);
    }
    let Ok(readiness) = crate::setup::database::assess(connection).await else {
        return Ok(DatabaseReadiness::PrerequisitesFailed);
    };
    if readiness.pending_migrations != 0
        || !crate::store::persistence::startup_health::schema_exists(connection).await?
    {
        return Ok(DatabaseReadiness::SchemaNotCurrent);
    }
    Ok(DatabaseReadiness::Current)
}

pub(super) async fn daemon(config: &ProxyConfig) -> Check {
    let Ok(endpoint) = crate::config::endpoint::inspect_url(config) else {
        return Check::new(
            "daemon",
            true,
            "fail",
            "HEALTH_ENDPOINT_INVALID",
            "Selected inspect listener is not a valid supported local endpoint; no request was sent.",
        );
    };
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return Check::new(
            "daemon",
            false,
            "unavailable",
            "HEALTH_CLIENT_UNAVAILABLE",
            "Read-only health probe could not be initialized.",
        );
    };
    let result = client.get(format!("{endpoint}/api/health")).send().await;
    let Ok(response) = result else {
        return Check::new(
            "daemon",
            false,
            "unavailable",
            "DAEMON_NOT_REACHABLE",
            "The selected daemon is stopped or unavailable; doctor did not start it.",
        );
    };
    if !response.status().is_success() {
        return Check::new(
            "daemon",
            false,
            "unavailable",
            "HEALTH_REJECTED",
            "Selected endpoint did not accept the health request.",
        );
    }
    match health_metadata(response).await {
        Ok(value) if value.get("build").is_some() && value.get("persistence").is_some() => {
            if value.get("status").and_then(|v| v.as_str()) == Some("ok") {
                Check::new(
                    "daemon",
                    false,
                    "pass",
                    "HEALTH_REPORTED_READY",
                    "Selected endpoint reports healthy; instance ownership is verified by lifecycle commands.",
                )
            } else {
                Check::new(
                    "daemon",
                    true,
                    "fail",
                    "HEALTH_DEGRADED",
                    "Selected daemon reports degraded health. Inspect the local health view; no captured content is included here.",
                )
            }
        }
        _ => Check::new(
            "daemon",
            false,
            "unavailable",
            "HEALTH_UNRECOGNIZED",
            "Selected endpoint did not return recognized Mike health metadata.",
        ),
    }
}

pub(super) async fn code_scan(config: &ProxyConfig) -> Check {
    let facts = fetch_code_scan(config).await;
    super::code_scan::probe_check(facts.as_ref())
}

async fn fetch_code_scan(config: &ProxyConfig) -> Option<super::code_scan::CodeScanFacts> {
    let endpoint = crate::config::endpoint::inspect_url(config).ok()?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()?;
    let response = client
        .get(format!("{endpoint}{}", super::code_scan::ROUTE))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body = health_metadata(response).await.ok()?;
    super::code_scan::CodeScanFacts::from_json(&body)
}

pub(super) async fn mcp(config: &ProxyConfig) -> Check {
    match crate::setup::mcp::check(config).await {
        Ok(probe) => {
            let ready = probe.handshake && probe.backend_read;
            let required = !probe.handshake || probe.code == "MCP_INSTANCE_MISMATCH";
            Check::new(
                "mcp_connection",
                required,
                if ready {
                    "pass"
                } else if required {
                    "fail"
                } else {
                    "unavailable"
                },
                probe.code,
                probe.detail,
            )
        }
        Err(_) => Check::new(
            "mcp_connection",
            true,
            "fail",
            "MCP_COMPANION_INVALID",
            "The matching installed companion could not be verified; paths and raw child output are omitted.",
        ),
    }
}

async fn health_metadata(mut response: reqwest::Response) -> anyhow::Result<serde_json::Value> {
    const MAX_HEALTH_BYTES: usize = 65536;
    anyhow::ensure!(
        response
            .content_length()
            .is_none_or(|length| length <= MAX_HEALTH_BYTES as u64),
        "health metadata exceeds diagnostic limit"
    );
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            body.len().saturating_add(chunk.len()) <= MAX_HEALTH_BYTES,
            "health metadata exceeds diagnostic limit"
        );
        body.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&body)?)
}
