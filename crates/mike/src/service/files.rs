use std::path::Path;

use super::error::ServiceError;

pub(crate) const MARKER: &str = "Managed by mike service install";
const MARKER_SCAN_LINES: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Existing {
    Missing,
    Ours(String),
    Foreign,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Presence {
    Missing,
    Ours,
    Foreign,
}

impl Existing {
    pub(crate) fn presence(&self) -> Presence {
        match self {
            Self::Missing => Presence::Missing,
            Self::Ours(_) => Presence::Ours,
            Self::Foreign => Presence::Foreign,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Written {
    Created,
    Replaced,
    Unchanged,
}

pub(crate) fn is_ours(content: &str) -> bool {
    content
        .lines()
        .take(MARKER_SCAN_LINES)
        .any(|line| line.contains(MARKER))
}

pub(crate) fn inspect(path: &Path) -> Result<Existing, ServiceError> {
    match std::fs::read_to_string(path) {
        Ok(content) if is_ours(&content) => Ok(Existing::Ours(content)),
        Ok(_) => Ok(Existing::Foreign),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Existing::Missing),
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => Ok(Existing::Foreign),
        Err(source) => Err(ServiceError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

pub(crate) fn write(
    path: &Path,
    content: &str,
    existing: &Existing,
) -> Result<Written, ServiceError> {
    match existing {
        Existing::Foreign => {
            return Err(ServiceError::ForeignFile {
                path: path.to_path_buf(),
            });
        }
        Existing::Ours(old) if old == content => return Ok(Written::Unchanged),
        Existing::Ours(_) | Existing::Missing => {}
    }
    let write_error = |source| ServiceError::Write {
        path: path.to_path_buf(),
        source,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(write_error)?;
    }
    crate::owner_only_file::write(path, content).map_err(write_error)?;
    Ok(match existing {
        Existing::Missing => Written::Created,
        _ => Written::Replaced,
    })
}

pub(crate) fn remove(path: &Path) -> Result<(), ServiceError> {
    std::fs::remove_file(path).map_err(|source| ServiceError::Remove {
        path: path.to_path_buf(),
        source,
    })
}
