use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;

pub const LOCK_NAME: &str = super::LOCK_NAME;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("managed build admission was cancelled before execution")]
    Cancelled,
    #[error("cooperative managed build admission is unsupported on this platform or filesystem")]
    Unsupported,
    #[error("unsafe managed build admission path `{path}`: {reason}")]
    UnsafePath { path: PathBuf, reason: &'static str },
    #[error("managed build admission could not {action} `{path}`: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
}

impl From<super::AdmissionError> for Error {
    fn from(error: super::AdmissionError) -> Self {
        match error {
            super::AdmissionError::Cancelled => Self::Cancelled,
            super::AdmissionError::Unsupported => Self::Unsupported,
            super::AdmissionError::UnsafePath { path, reason } => Self::UnsafePath { path, reason },
            super::AdmissionError::Io {
                action,
                path,
                source,
            } => Self::Io {
                action,
                path,
                source,
            },
        }
    }
}

#[derive(Debug)]
#[must_use = "retain this permit until the owned process group and output cleanup have finished"]
pub struct Permit {
    _permit: super::Permit,
}

pub async fn acquire(
    private_state_root: &Path,
    cancellation: &CancellationToken,
) -> Result<Permit, Error> {
    super::acquire(private_state_root, cancellation)
        .await
        .map(|permit| Permit { _permit: permit })
        .map_err(Error::from)
}

pub fn io_error(action: &'static str, path: &Path, source: std::io::Error) -> Error {
    super::io_error(action, path, source).into()
}
