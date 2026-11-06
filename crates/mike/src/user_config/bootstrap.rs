use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::file::{ConfigFile, Entry, Setting};
use super::paths::default_config_file;

const PREFIX: &str = "MIKE_";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    Environment,
    DotEnv,
    ConfigFile,
}

impl Origin {
    fn label(self) -> &'static str {
        match self {
            Origin::Environment => "environment",
            Origin::DotEnv => ".env in the working directory",
            Origin::ConfigFile => "config file",
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
    provenance().is_some_and(|p| {
        matches!(
            p.origins.get(env_name),
            Some(Origin::Environment | Origin::DotEnv)
        )
    })
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

pub fn load_environment() {
    let path = default_config_file();
    let before = snapshot();
    let after_dotenv = snapshot();
    let applied = apply_config_file(path.as_deref().map_err(ToString::to_string));
    let origins = merge_origins(&before, &after_dotenv, &applied.assigned);
    let _ = PROVENANCE.set(Provenance {
        file: path.ok(),
        state: applied.state,
        origins,
    });
}

pub fn require_valid_config_file() -> anyhow::Result<()> {
    match provenance().map(|p| &p.state) {
        Some(FileState::Unusable(why)) => anyhow::bail!("{why}"),
        _ => Ok(()),
    }
}

pub(crate) struct Applied {
    pub(crate) state: FileState,
    pub(crate) assigned: Vec<String>,
}

pub(crate) fn apply_config_file(path: Result<&Path, String>) -> Applied {
    let nothing = |state| Applied {
        state,
        assigned: Vec::new(),
    };
    let path = match path {
        Ok(path) => path,
        Err(why) => return nothing(FileState::NoLocation(why)),
    };
    if !path.exists() {
        return nothing(FileState::Missing);
    }
    let entries = match ConfigFile::open(path).and_then(|f| f.entries()) {
        Ok(entries) => entries,
        Err(e) => return unusable(&e),
    };
    let assigned = plan(&entries, &|name| std::env::var(name).ok());
    for (name, value) in &assigned {
        unsafe { std::env::set_var(name, value) };
    }
    Applied {
        state: FileState::Loaded {
            keys: entries.len(),
        },
        assigned: assigned.into_iter().map(|(name, _)| name).collect(),
    }
}

fn unusable(error: &super::UserConfigError) -> Applied {
    let why = format!("{error}");
    eprintln!("mike: {why}");
    Applied {
        state: FileState::Unusable(why),
        assigned: Vec::new(),
    }
}

pub(crate) fn plan(
    entries: &[Entry],
    existing: &dyn Fn(&str) -> Option<String>,
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let taken = |out: &[(String, String)], name: &str| {
        existing(name).is_some() || out.iter().any(|(n, _)| n == name)
    };
    for entry in entries {
        if let Setting::Literal(value) = &entry.setting
            && !value.trim().is_empty()
            && !taken(&out, &entry.env_name)
        {
            out.push((entry.env_name.clone(), value.clone()));
        }
    }
    for entry in entries {
        if let Setting::FromEnv(source) = &entry.setting
            && let Some(value) = existing(source).filter(|v| !v.trim().is_empty())
            && !taken(&out, &entry.env_name)
        {
            out.push((entry.env_name.clone(), value));
        }
    }
    out
}

fn merge_origins(
    before: &EnvSnapshot,
    after_dotenv: &EnvSnapshot,
    from_file: &[String],
) -> BTreeMap<String, Origin> {
    let mut origins = BTreeMap::new();
    for (name, value) in after_dotenv {
        let origin = if before.get(name) == Some(value) {
            Origin::Environment
        } else {
            Origin::DotEnv
        };
        origins.insert(name.clone(), origin);
    }
    for name in from_file {
        origins.insert(name.clone(), Origin::ConfigFile);
    }
    origins
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
