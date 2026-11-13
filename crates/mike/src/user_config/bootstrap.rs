use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::paths::default_config_file;

const PREFIX: &str = "MIKE_";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    Environment,
}

impl Origin {
    fn label(self) -> &'static str {
        match self {
            Origin::Environment => "environment",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FileState {
    NoLocation(String),
    Missing,
    Loaded { keys: usize },
    Unusable(String),
}

pub(crate) struct Provenance {
    pub(crate) file: Option<PathBuf>,
    pub(crate) state: FileState,
    pub(crate) origins: BTreeMap<String, Origin>,
}

static PROVENANCE: OnceLock<Provenance> = OnceLock::new();

pub(crate) fn provenance() -> Option<&'static Provenance> {
    PROVENANCE.get()
}

pub(crate) fn overridden_by_environment(env_name: &str) -> bool {
    provenance().is_some_and(|p| matches!(p.origins.get(env_name), Some(Origin::Environment)))
}

pub(crate) fn resolved_config_file() -> Result<PathBuf, super::UserConfigError> {
    match provenance().and_then(|p| p.file.clone()) {
        Some(path) => Ok(path),
        None => default_config_file(),
    }
}

type EnvSnapshot = BTreeMap<String, String>;

fn snapshot() -> EnvSnapshot {
    std::env::vars()
        .filter(|(name, _)| name.starts_with(PREFIX))
        .collect()
}

pub fn observe_selected(selected: Option<&Path>) -> anyhow::Result<()> {
    let paths = crate::config::paths::InstallationPaths::resolve(selected)?;
    let file = paths.config_file;
    let state = if !file.exists() {
        FileState::Missing
    } else {
        match crate::config::load::file::read(&file) {
            Ok(table) => FileState::Loaded { keys: table.len() },
            Err(_) => FileState::Unusable("selected configuration could not be loaded".into()),
        }
    };
    let origins = snapshot()
        .into_keys()
        .map(|key| (key, Origin::Environment))
        .collect();
    let _ = PROVENANCE.set(Provenance {
        file: Some(file),
        state,
        origins,
    });
    Ok(())
}

pub fn require_valid_config_file() -> anyhow::Result<()> {
    match provenance().map(|p| &p.state) {
        Some(FileState::Unusable(why)) => anyhow::bail!("{why}"),
        _ => Ok(()),
    }
}

pub(crate) fn render_sources() -> String {
    match provenance() {
        Some(p) => render_provenance(p),
        None => "# CONFIG FILE: not loaded in this process\n".to_string(),
    }
}

pub(crate) fn render_provenance(p: &Provenance) -> String {
    let mut out = match (&p.file, &p.state) {
        (_, FileState::NoLocation(why)) => format!("# CONFIG FILE: location unknown ({why})\n"),
        (Some(path), FileState::Missing) => {
            format!("# CONFIG FILE: {} (does not exist)\n", path.display())
        }
        (Some(path), FileState::Loaded { keys }) => {
            format!("# CONFIG FILE: {} ({keys} keys)\n", path.display())
        }
        (Some(path), FileState::Unusable(why)) => format!(
            "# CONFIG FILE: {} is unusable ({})\n",
            path.display(),
            why.replace('\n', " ")
        ),
        (None, _) => "# CONFIG FILE: location unknown\n".to_string(),
    };
    out.push_str("# SOURCES of the MIKE_* variables in effect (values are not shown):\n");
    for (name, origin) in &p.origins {
        out.push_str(&format!("#   {name}: {}\n", origin.label()));
    }
    out
}
