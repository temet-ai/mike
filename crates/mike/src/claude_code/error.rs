use std::io;
use std::path::PathBuf;

use crate::user_config::UserConfigError;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ClaudeCodeError {
    #[error("HOME is not set, so mike cannot tell where Claude Code keeps its files")]
    NoHome,
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path} is not valid JSON ({message}), so mike left it as it is")]
    Parse { path: PathBuf, message: String },
    #[error("{path} holds {found} where mike needs {expected}, so mike left it as it is")]
    Shape {
        path: PathBuf,
        found: String,
        expected: &'static str,
    },
    #[error("cannot write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "`mike-mcp` is neither beside `mike` nor on PATH. Install it with `brew install temet-ai/mike/mike-mcp`, or build it with `make install` in a checkout"
    )]
    NoMcpBinary,
    #[error("mike-mcp did not answer: {0}")]
    Probe(String),
    #[error("`{name}` cannot be the name of the shell function: {reason}")]
    BadFunctionName { name: String, reason: String },
    #[error(
        "{path} has the line `{marker}` without a matching end line, so mike left the file as it is. Fix or remove that line and run the command again"
    )]
    BrokenBlock { path: PathBuf, marker: &'static str },
    #[error(
        "no folder of mods was found beside this mike binary or in a checkout. A Homebrew install keeps them in the `share/mike/mods` folder under the Homebrew prefix"
    )]
    NoModsSource,
    #[error("{action} is switched off while {} is set", crate::sandbox::ENV_VAR)]
    Sandbox { action: &'static str },
    #[error("{0}")]
    Config(#[from] UserConfigError),
    #[error("the configuration could not be resolved: {0}")]
    Load(String),
    #[error("{0}")]
    Other(String),
}
