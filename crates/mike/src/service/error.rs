use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ServiceError {
    #[error(
        "background services are supported on macOS (launchd) and Linux (systemd user units); this system is {os}"
    )]
    UnsupportedPlatform { os: String },
    #[error("{action} is switched off while {} is set", crate::sandbox::ENV_VAR)]
    Sandbox { action: &'static str },
    #[error(
        "HOME is not set, so mike cannot tell where the per-user service files and state belong"
    )]
    NoHome,
    #[error("the configuration could not be resolved: {0}")]
    Config(String),
    #[error("cannot find the running mike binary: {0}")]
    Binary(#[source] io::Error),
    #[error(
        "the developer stack is running ({units}). A background service would fight it for the same ports and the same database, so nothing was written. Keep using the developer stack, or stop it first"
    )]
    DevStackActive { units: String },
    #[error(
        "something is already listening on the {what} address {addr}, so nothing was written. It may be the developer stack or a proxy that `mike claude` started in the background. Stop it, or choose other addresses in the config file, and run `mike service install` again"
    )]
    PortInUse { what: &'static str, addr: String },
    #[error(
        "{path} exists but mike did not write it, so mike leaves it alone. Move it away and run the command again"
    )]
    ForeignFile { path: PathBuf },
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot remove {path}: {source}")]
    Remove {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("`{command}` failed: {detail}")]
    CommandFailed { command: String, detail: String },
    #[error("could not run `{program}`: {source}")]
    Spawn {
        program: String,
        #[source]
        source: io::Error,
    },
    #[error("the {name} service is not installed. Run `mike service install` first")]
    NotInstalled { name: &'static str },
    #[error("{0}")]
    Unhealthy(String),
}
