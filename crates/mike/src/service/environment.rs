use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::binary;
use super::error::ServiceError;
use super::spec::{ServiceKind, Spec};

const HOMEBREW_BIN_DIRS: [&str; 5] = [
    "/opt/homebrew/bin",
    "/opt/homebrew/sbin",
    "/usr/local/bin",
    "/usr/local/sbin",
    "/home/linuxbrew/.linuxbrew/bin",
];
const SYSTEM_BIN_DIRS: [&str; 4] = ["/usr/bin", "/bin", "/usr/sbin", "/sbin"];
const BACKGROUND_MODEL_TOOLS: [&str; 2] = ["claude", "codex"];

#[derive(Debug, Clone)]
pub(crate) struct Inputs {
    pub(crate) home: PathBuf,
    pub(crate) exe: PathBuf,
    pub(crate) xdg_config_home: Option<String>,
    pub(crate) xdg_state_home: Option<String>,
    pub(crate) mise_data_dir: Option<String>,
    pub(crate) path_var: Option<OsString>,
}

impl Inputs {
    pub(crate) fn from_process() -> Result<Self, ServiceError> {
        let home = std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)
            .ok_or(ServiceError::NoHome)?;
        let current = std::env::current_exe().map_err(ServiceError::Binary)?;
        Ok(Self {
            home,
            exe: binary::stable_path(&current)?,
            xdg_config_home: std::env::var("XDG_CONFIG_HOME").ok(),
            xdg_state_home: std::env::var("XDG_STATE_HOME").ok(),
            mise_data_dir: std::env::var("MISE_DATA_DIR").ok(),
            path_var: std::env::var_os("PATH"),
        })
    }
}

fn absolute(value: Option<&str>) -> Option<&str> {
    value.filter(|v| Path::new(v).is_absolute())
}

fn mise_shims(inputs: &Inputs) -> PathBuf {
    match absolute(inputs.mise_data_dir.as_deref()) {
        Some(data) => Path::new(data).join("shims"),
        None => inputs.home.join(".local/share/mise/shims"),
    }
}

fn tool_dirs(inputs: &Inputs) -> Vec<PathBuf> {
    BACKGROUND_MODEL_TOOLS
        .iter()
        .filter_map(|tool| {
            crate::launch::cli::locate_executable(tool, inputs.path_var.as_deref())
                .and_then(|found| found.parent().map(Path::to_path_buf))
        })
        .collect()
}

pub(crate) fn service_path(inputs: &Inputs) -> String {
    let mut dirs: Vec<PathBuf> = vec![inputs.home.join(".local/bin"), mise_shims(inputs)];
    dirs.extend(HOMEBREW_BIN_DIRS.iter().map(PathBuf::from));
    dirs.extend(tool_dirs(inputs));
    dirs.extend(SYSTEM_BIN_DIRS.iter().map(PathBuf::from));
    let mut seen: Vec<PathBuf> = Vec::new();
    for dir in dirs {
        if dir.is_absolute() && !seen.contains(&dir) {
            seen.push(dir);
        }
    }
    seen.iter()
        .map(|d| d.display().to_string())
        .collect::<Vec<_>>()
        .join(":")
}

pub(crate) fn service_environment(inputs: &Inputs) -> Vec<(String, String)> {
    let mut environment = vec![("PATH".to_string(), service_path(inputs))];
    for (name, value) in [
        ("XDG_CONFIG_HOME", &inputs.xdg_config_home),
        ("XDG_STATE_HOME", &inputs.xdg_state_home),
    ] {
        if let Some(value) = absolute(value.as_deref()) {
            environment.push((name.to_string(), value.to_string()));
        }
    }
    environment
}

pub(crate) fn specs(inputs: &Inputs, state_dir: &Path) -> Vec<Spec> {
    let environment = service_environment(inputs);
    ServiceKind::ALL
        .iter()
        .map(|kind| Spec {
            kind: *kind,
            exe: inputs.exe.clone(),
            state_dir: state_dir.to_path_buf(),
            environment: environment.clone(),
        })
        .collect()
}
