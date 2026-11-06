use std::fmt;

#[derive(Debug)]
pub(crate) enum AnthropicCliError {
    NotFound,
    Timeout,
    Io(String),
    Failed(String),
    AlreadyRunning(String),
    NotRunning,
}

impl fmt::Display for AnthropicCliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "claude CLI binary not found on PATH"),
            Self::Timeout => write!(f, "claude CLI command timed out"),
            Self::Io(e) => write!(f, "claude CLI I/O error: {e}"),
            Self::Failed(e) => write!(f, "claude CLI command failed: {e}"),
            Self::AlreadyRunning(id) => write!(f, "a sign-in is already in progress ({id})"),
            Self::NotRunning => write!(f, "no sign-in is in progress"),
        }
    }
}

impl std::error::Error for AnthropicCliError {}
