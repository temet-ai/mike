use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use serde::de::DeserializeOwned;
use uuid::Uuid;

use super::model::Recipe;
use super::owned::atomic_write;

#[derive(Debug, Clone)]
pub struct Environment {
    pub home: PathBuf,
    pub root: PathBuf,
    pub machine_uuid: Uuid,
    pub machine_name: String,
}

impl Environment {
    pub(super) fn state_dir(&self) -> PathBuf {
        self.home.join(".local/state/mike-learning-promotion")
    }

    pub(super) fn skill_roots(&self) -> Vec<PathBuf> {
        vec![
            self.home.join(".codex/skills"),
            self.home.join(".claude/skills"),
        ]
    }

    pub(super) fn installed(&self) -> bool {
        let parent = self
            .root
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == "share");
        parent && self.root.file_name().is_some_and(|name| name == "mike")
    }

    pub(super) fn data_root(&self) -> PathBuf {
        if self.installed() {
            self.home.join(".local/state/mike/learning")
        } else {
            self.root.clone()
        }
    }

    pub(super) fn runner(&self) -> PathBuf {
        self.root.join("scripts/tool-reuse")
    }

    pub(super) fn recipes_dir(&self) -> PathBuf {
        match std::env::var_os("MIKE_TOOL_REUSE_STATE") {
            Some(state) if !state.is_empty() => PathBuf::from(state).join("recipes"),
            _ => self.home.join(".local/state/mike-tool-reuse/recipes"),
        }
    }
}

pub(super) fn load_json<T: DeserializeOwned + Default>(path: &Path) -> Result<T> {
    if !path.exists() {
        return Ok(T::default());
    }
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

pub(super) fn store_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = serde_json::to_string_pretty(value)?;
    atomic_write(path, &format!("{text}\n")).with_context(|| format!("writing {}", path.display()))
}

pub(super) fn load_recipes(directory: &Path) -> Vec<Recipe> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    files.sort();
    files
        .iter()
        .filter_map(|file| serde_json::from_str(&fs::read_to_string(file).ok()?).ok())
        .collect()
}

pub fn acquire_lock(home: &Path) -> Result<fs::File> {
    let path = home.join(".local/state/mike-learning-promotion.lock");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive).map_err(
        |error| {
            anyhow!(
                "another learning promotion run holds {}: {error}",
                path.display()
            )
        },
    )?;
    Ok(file)
}
