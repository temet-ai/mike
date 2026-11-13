mod bootstrap;
mod command;
mod file;
mod import;
pub(crate) mod paths;

#[cfg(test)]
mod tests;

pub(crate) use bootstrap::{
    FileState, overridden_by_environment, provenance, render_sources, resolved_config_file,
};
pub use bootstrap::{observe_selected, require_valid_config_file};
pub use command::{ImportEnvArgs, UserConfigArgs, UserConfigCommand, run};
#[cfg(test)]
pub(crate) use file::Setting;
pub(crate) use file::{ConfigFile, SettingValue, env_name_for};
pub(crate) use paths::state_dir;

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub(crate) enum UserConfigError {
    #[error(
        "neither XDG_CONFIG_HOME nor HOME is set, so the location of the mike config file is unknown"
    )]
    NoConfigDir,
    #[error("neither XDG_STATE_HOME nor HOME is set, so the mike state directory is unknown")]
    NoStateDir,
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not valid TOML: {message}")]
    Parse { path: PathBuf, message: String },
    #[error(
        "key `{key}` in {path} cannot be used: {reason}. Keys are the MIKE_* variable names \
         without the MIKE_ prefix, in lower case (`database_url` for MIKE_DATABASE_URL)"
    )]
    BadKey {
        path: PathBuf,
        key: String,
        reason: String,
    },
    #[error(
        "the value of `{key}` in {path} is a {kind}; use a string, number, boolean or an array of those"
    )]
    UnsupportedValue {
        path: PathBuf,
        key: String,
        kind: &'static str,
    },
    #[error("cannot write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
