use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::store::persistence::project_knowledge::{MEMORY_SOURCE, SKILLS_SOURCE};

const CONFIG_PATH: &str = ".mike/knowledge.json";

#[derive(Debug, PartialEq, Eq)]
pub(super) struct DeclaredSource {
    pub name: String,
    pub root: PathBuf,
    pub excludes: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum DeclaredError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not a valid knowledge declaration: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("{path} declares source {name:?} with {reason}")]
    Invalid {
        path: PathBuf,
        name: String,
        reason: &'static str,
    },
}

#[derive(Deserialize)]
struct Config {
    #[serde(default)]
    sources: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    name: String,
    path: String,
    #[serde(default)]
    exclude: Vec<String>,
}

fn stays_inside(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

fn reject(entry: &Entry) -> Option<&'static str> {
    if entry.name.trim().is_empty() {
        return Some("an empty name");
    }
    if entry.name == MEMORY_SOURCE || entry.name == SKILLS_SOURCE {
        return Some("a name reserved for the proxy's own scan");
    }
    if !stays_inside(&entry.path) {
        return Some("a path that is not relative to the repository");
    }
    if !entry.exclude.iter().all(|exclude| stays_inside(exclude)) {
        return Some("an exclude that is not relative to the source");
    }
    None
}

pub(super) fn parse(
    location_root: &Path,
    config_path: &Path,
    text: &str,
) -> Result<Vec<DeclaredSource>, DeclaredError> {
    let config: Config = serde_json::from_str(text).map_err(|source| DeclaredError::Parse {
        path: config_path.to_path_buf(),
        source,
    })?;
    config
        .sources
        .into_iter()
        .map(|entry| match reject(&entry) {
            Some(reason) => Err(DeclaredError::Invalid {
                path: config_path.to_path_buf(),
                name: entry.name,
                reason,
            }),
            None => Ok(DeclaredSource {
                root: location_root.join(&entry.path),
                name: entry.name,
                excludes: entry.exclude,
            }),
        })
        .collect()
}

pub(super) async fn read(location_root: &str) -> Result<Vec<DeclaredSource>, DeclaredError> {
    let root = Path::new(location_root);
    let config_path = root.join(CONFIG_PATH);
    match tokio::fs::read_to_string(&config_path).await {
        Ok(text) => parse(root, &config_path, &text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(source) => Err(DeclaredError::Io {
            path: config_path,
            source,
        }),
    }
}
