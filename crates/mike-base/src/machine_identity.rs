use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod merge;

pub use merge::merge_legacy_at_startup;

const FILE_NAME: &str = "machine.json";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct MachineIdentity {
    pub machine_uuid: Uuid,
    pub machine_name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum MachineIdentityError {
    #[error("neither XDG_STATE_HOME nor HOME is set, so the mike state directory is unknown")]
    NoStateDir,
    #[error("no machine identity file at {path}")]
    Missing { path: PathBuf },
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not a machine identity file: {source}")]
    Malformed {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

pub fn state_dir(
    xdg_state_home: Option<&str>,
    home: Option<&str>,
) -> Result<PathBuf, MachineIdentityError> {
    let base = match xdg_state_home.filter(|value| !value.is_empty()) {
        Some(xdg) => PathBuf::from(xdg),
        None => PathBuf::from(
            home.filter(|value| !value.is_empty())
                .ok_or(MachineIdentityError::NoStateDir)?,
        )
        .join(".local")
        .join("state"),
    };
    Ok(base.join("mike"))
}

pub fn default_state_dir() -> Result<PathBuf, MachineIdentityError> {
    crate::config::paths::InstallationPaths::resolve(None)
        .map(|paths| paths.state_dir)
        .map_err(|_| MachineIdentityError::NoStateDir)
}

pub fn file_in(state_dir: &Path) -> PathBuf {
    state_dir.join(FILE_NAME)
}

pub async fn read_file(
    path: &Path,
) -> Result<Option<MachineIdentity>, MachineIdentityError> {
    match tokio::fs::read(path).await {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|source| {
            MachineIdentityError::Malformed {
                path: path.to_path_buf(),
                source,
            }
        }),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(source) => Err(MachineIdentityError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

pub async fn load_from(state_dir: &Path) -> Result<MachineIdentity, MachineIdentityError> {
    let path = file_in(state_dir);
    read_file(&path)
        .await?
        .ok_or(MachineIdentityError::Missing { path })
}

pub async fn load_default() -> Result<MachineIdentity, MachineIdentityError> {
    load_or_create_in(&default_state_dir()?).await
}

pub async fn load_or_create_in(
    state_dir: &Path,
) -> Result<MachineIdentity, MachineIdentityError> {
    let path = file_in(state_dir);
    if let Some(identity) = read_file(&path).await? {
        return Ok(identity);
    }
    let fresh = MachineIdentity {
        machine_uuid: Uuid::new_v4(),
        machine_name: host_name().await,
    };
    create_once(state_dir, &path, &fresh).await?;
    read_file(&path)
        .await?
        .ok_or(MachineIdentityError::Missing { path })
}

async fn host_name() -> String {
    tokio::process::Command::new("hostname")
        .output()
        .await
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

async fn create_once(
    state_dir: &Path,
    path: &Path,
    identity: &MachineIdentity,
) -> Result<(), MachineIdentityError> {
    let io = |source| MachineIdentityError::Io {
        path: path.to_path_buf(),
        source,
    };
    tokio::fs::create_dir_all(state_dir).await.map_err(io)?;
    let mut body =
        serde_json::to_vec_pretty(identity).map_err(|source| MachineIdentityError::Malformed {
            path: path.to_path_buf(),
            source,
        })?;
    body.push(b'\n');
    let staged = state_dir.join(format!("{FILE_NAME}.{}.tmp", identity.machine_uuid));
    tokio::fs::write(&staged, &body).await.map_err(io)?;
    let linked = tokio::fs::hard_link(&staged, path).await;
    let _ = tokio::fs::remove_file(&staged).await;
    match linked {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(source) => Err(io(source)),
    }
}

#[cfg(test)]
mod tests;
