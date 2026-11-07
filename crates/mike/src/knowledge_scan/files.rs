use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::frontmatter;
use crate::store::persistence::project_knowledge::DocumentInput;

const MAX_DOCUMENTS: usize = 5000;
const MEMORY_INDEX_FILE: &str = "MEMORY.md";
const SKILL_FILE: &str = "SKILL.md";
const SKIPPED_DIRECTORIES: [&str; 4] = [".git", "node_modules", "target", ".venv"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceKind {
    Memory,
    Skills,
    Tree,
}

impl SourceKind {
    fn default_kind(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Skills => "skill",
            Self::Tree => "artifact",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum ScanError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not valid UTF-8")]
    NotUtf8 { path: PathBuf },
    #[error("{path} changed while it was being read; trying again next cycle")]
    Changed { path: PathBuf },
    #[error("{root} holds {count} documents, more than the {MAX_DOCUMENTS} one source may carry")]
    TooMany { root: PathBuf, count: usize },
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> ScanError + '_ {
    move |source| ScanError::Io {
        path: path.to_path_buf(),
        source,
    }
}

pub(super) fn collect(root: &Path, kind: SourceKind) -> Result<Vec<DocumentInput>, ScanError> {
    collect_excluding(root, kind, &[])
}

pub(super) fn collect_excluding(
    root: &Path,
    kind: SourceKind,
    excludes: &[String],
) -> Result<Vec<DocumentInput>, ScanError> {
    let mut paths = match kind {
        SourceKind::Memory => memory_paths(root)?,
        SourceKind::Skills => skill_paths(root)?,
        SourceKind::Tree => tree_paths(root, excludes)?,
    };
    paths.sort();
    if paths.len() > MAX_DOCUMENTS {
        return Err(ScanError::TooMany {
            root: root.to_path_buf(),
            count: paths.len(),
        });
    }
    paths
        .iter()
        .map(|path| read_document(root, path, kind))
        .collect()
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

fn memory_paths(root: &Path) -> Result<Vec<PathBuf>, ScanError> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).map_err(io_error(&directory))? {
            let entry = entry.map_err(io_error(&directory))?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(io_error(&path))?;
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file()
                && is_markdown(&path)
                && path.file_name().and_then(|name| name.to_str()) != Some(MEMORY_INDEX_FILE)
            {
                found.push(path);
            }
        }
    }
    Ok(found)
}

fn tree_paths(root: &Path, excludes: &[String]) -> Result<Vec<PathBuf>, ScanError> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).map_err(io_error(&directory))? {
            let entry = entry.map_err(io_error(&directory))?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(io_error(&path))?;
            if is_excluded(root, &path, excludes) {
                continue;
            }
            if file_type.is_dir() {
                let name = entry.file_name();
                if !name
                    .to_str()
                    .is_some_and(|name| SKIPPED_DIRECTORIES.contains(&name))
                {
                    pending.push(path);
                }
            } else if file_type.is_file() && is_markdown(&path) {
                found.push(path);
            }
        }
    }
    Ok(found)
}

fn is_excluded(root: &Path, path: &Path, excludes: &[String]) -> bool {
    path.strip_prefix(root)
        .is_ok_and(|relative| excludes.iter().any(|exclude| relative.starts_with(exclude)))
}

fn skill_paths(root: &Path) -> Result<Vec<PathBuf>, ScanError> {
    let mut found = Vec::new();
    for entry in fs::read_dir(root).map_err(io_error(root))? {
        let entry = entry.map_err(io_error(root))?;
        let skill = entry.path().join(SKILL_FILE);
        if entry.path().is_dir() && skill.is_file() {
            found.push(skill);
        }
    }
    Ok(found)
}

fn read_document(root: &Path, path: &Path, kind: SourceKind) -> Result<DocumentInput, ScanError> {
    let before = fs::metadata(path).map_err(io_error(path))?;
    let bytes = fs::read(path).map_err(io_error(path))?;
    let after = fs::metadata(path).map_err(io_error(path))?;
    if before.modified().ok() != after.modified().ok() || before.len() != after.len() {
        return Err(ScanError::Changed {
            path: path.to_path_buf(),
        });
    }
    let body = String::from_utf8(bytes).map_err(|_| ScanError::NotUtf8 {
        path: path.to_path_buf(),
    })?;
    let metadata = frontmatter::parse(&body);
    let relative_path = relative(root, path);
    Ok(DocumentInput {
        title: title_of(&metadata, &body, &relative_path, kind),
        kind: kind_of(&metadata, kind),
        status: text_field(&metadata, "status").unwrap_or_else(|| "unknown".to_owned()),
        content_hash: hex_digest(body.as_bytes()),
        metadata: Value::Object(metadata),
        relative_path,
        body,
    })
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn text_field(metadata: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn nested_type(metadata: &serde_json::Map<String, Value>) -> Option<String> {
    metadata
        .get("metadata")
        .and_then(Value::as_object)
        .and_then(|inner| text_field(inner, "type"))
}

fn kind_of(metadata: &serde_json::Map<String, Value>, kind: SourceKind) -> String {
    text_field(metadata, "kind")
        .or_else(|| text_field(metadata, "type"))
        .or_else(|| nested_type(metadata))
        .unwrap_or_else(|| kind.default_kind().to_owned())
}

fn title_of(
    metadata: &serde_json::Map<String, Value>,
    body: &str,
    relative_path: &str,
    kind: SourceKind,
) -> String {
    let declared = match kind {
        SourceKind::Tree => text_field(metadata, "title"),
        SourceKind::Memory | SourceKind::Skills => text_field(metadata, "name"),
    };
    if let Some(name) = declared {
        return name;
    }
    if let Some(heading) = body.lines().find_map(|line| line.strip_prefix("# ")) {
        return heading.trim().to_owned();
    }
    let path = Path::new(relative_path);
    let from_directory = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str());
    let from_file = path.file_stem().and_then(|stem| stem.to_str());
    from_directory
        .filter(|_| path.file_name().and_then(|name| name.to_str()) == Some(SKILL_FILE))
        .or(from_file)
        .unwrap_or(relative_path)
        .to_owned()
}

pub(super) fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
