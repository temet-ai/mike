use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::UserConfigError;
use super::bootstrap::{FileState, Origin, Provenance, apply_config_file, plan, render_provenance};
use super::file::{ConfigFile, Entry, Setting, SettingValue};
use super::import::{import_env, parse_dotenv};
use super::paths::{config_dir_from, config_file_from, state_dir_from};
use crate::config::CliOverrides;
use crate::environment::{Environment, MapEnvironment};

fn temp_file(contents: &str) -> anyhow::Result<(tempfile::TempDir, PathBuf)> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    std::fs::write(&path, contents)?;
    Ok((dir, path))
}

fn entries_of(contents: &str) -> anyhow::Result<Vec<Entry>> {
    let (_dir, path) = temp_file(contents)?;
    Ok(ConfigFile::open(&path)?.entries()?)
}

fn literal(env_name: &str, value: &str) -> Entry {
    Entry {
        env_name: env_name.to_string(),
        setting: Setting::Literal(value.to_string()),
    }
}

fn from_env(env_name: &str, source: &str) -> Entry {
    Entry {
        env_name: env_name.to_string(),
        setting: Setting::FromEnv(source.to_string()),
    }
}

mod apply;
mod file;
mod import;
mod paths;
