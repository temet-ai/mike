use std::path::{Path, PathBuf};

use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::machine_identity::{self, MachineIdentity, MachineIdentityError};

const SUPPORTED_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
struct KnowledgeConfig {
    version: u32,
    project_uuid: Uuid,
}

#[derive(Debug, Deserialize)]
struct LocationResponse {
    location_uuid: Uuid,
}

#[derive(Debug, thiserror::Error)]
pub enum KnowledgeConfigError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not valid knowledge configuration: {source}")]
    Malformed {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("{path} declares version {found}; this mike understands version {SUPPORTED_VERSION}")]
    UnsupportedVersion { path: PathBuf, found: u32 },
}

#[derive(Debug, PartialEq, Eq)]
pub enum LocationOutcome {
    NotConfigured,
    Bound(Uuid),
    Conflict(String),
    Unavailable(String),
}

pub async fn read_project_uuid(
    repo_root: &Path,
) -> Result<Option<Uuid>, KnowledgeConfigError> {
    let path = repo_root.join(".mike").join("knowledge.json");
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(KnowledgeConfigError::Io { path, source }),
    };
    let config: KnowledgeConfig =
        serde_json::from_slice(&bytes).map_err(|source| KnowledgeConfigError::Malformed {
            path: path.clone(),
            source,
        })?;
    if config.version != SUPPORTED_VERSION {
        return Err(KnowledgeConfigError::UnsupportedVersion {
            path,
            found: config.version,
        });
    }
    Ok(Some(config.project_uuid))
}

pub async fn resolve_location(
    inspect_addr: &str,
    repo_root: &Path,
    state_dir: Option<&Path>,
) -> LocationOutcome {
    let project = match read_project_uuid(repo_root).await {
        Ok(Some(project)) => project,
        Ok(None) => return LocationOutcome::NotConfigured,
        Err(error) => return LocationOutcome::Unavailable(error.to_string()),
    };
    let identity = match load_identity(state_dir).await {
        Ok(identity) => identity,
        Err(error) => return LocationOutcome::Unavailable(error.to_string()),
    };
    let root = tokio::fs::canonicalize(repo_root)
        .await
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let body = json!({
        "project": project,
        "machine_uuid": identity.machine_uuid,
        "machine_name": identity.machine_name,
        "root_path": root.to_string_lossy(),
    });
    post_location(inspect_addr, &body).await
}

async fn load_identity(state_dir: Option<&Path>) -> Result<MachineIdentity, MachineIdentityError> {
    match state_dir {
        Some(dir) => machine_identity::load_from(dir).await,
        None => Err(MachineIdentityError::NoStateDir),
    }
}

async fn post_location(inspect_addr: &str, body: &Value) -> LocationOutcome {
    let url = format!("http://{inspect_addr}/api/knowledge/locations");
    let response = match reqwest::Client::new().post(&url).json(body).send().await {
        Ok(response) => response,
        Err(error) => return LocationOutcome::Unavailable(format!("{url}: {error}")),
    };
    let status = response.status();
    let text = match response.text().await {
        Ok(text) => text,
        Err(error) => return LocationOutcome::Unavailable(format!("{url}: {error}")),
    };
    if status == StatusCode::CONFLICT {
        return LocationOutcome::Conflict(text);
    }
    if !status.is_success() {
        return LocationOutcome::Unavailable(format!("{url} answered {status}: {text}"));
    }
    match serde_json::from_str::<LocationResponse>(&text) {
        Ok(parsed) => LocationOutcome::Bound(parsed.location_uuid),
        Err(error) => LocationOutcome::Unavailable(format!("{url} answered {text}: {error}")),
    }
}

#[cfg(test)]
pub mod tests;
