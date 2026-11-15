use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::frontmatter;
use crate::store::persistence::project_knowledge::DocumentInput;

const MAX_DOCUMENTS: usize = 5000;
const MEMORY_INDEX_FILE: &str = "MEMORY.md";
const SKILL_FILE: &str = "SKILL.md";
const WORKLOG_FILE: &str = "worklog.md";
const THREADS_DIRECTORY: &str = "threads";
const SKIPPED_DIRECTORIES: [&str; 4] = [".git", "node_modules", "target", ".venv"];
const GENERATED_MARKER: &str = "<!-- mike-learning-generated:v1 -->";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Memory,
    Skills,
    Tree,
    Instructions,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Scope {
    pub excludes: Vec<String>,
    pub includes: Vec<String>,
    pub extras: Vec<PathBuf>,
}

impl SourceKind {
    fn default_kind(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Skills => "skill",
            Self::Tree => "artifact",
            Self::Instructions => "instruction",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
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

pub fn collect_excluding(
    root: &Path,
    kind: SourceKind,
    excludes: &[String],
) -> Result<Vec<DocumentInput>, ScanError> {
    let scope = Scope {
        excludes: excludes.to_vec(),
        ..Scope::default()
    };
    collect(root, kind, &scope)
}

pub fn collect(
    root: &Path,
    kind: SourceKind,
    scope: &Scope,
) -> Result<Vec<DocumentInput>, ScanError> {
    let mut paths = match kind {
        SourceKind::Memory => memory_paths(root)?,
        SourceKind::Skills => skill_paths(root)?,
        SourceKind::Tree | SourceKind::Instructions if scope.includes.is_empty() => {
            tree_paths(root, &scope.excludes)?
        }
        SourceKind::Tree | SourceKind::Instructions => included_paths(root, &scope.includes)?,
    };
    paths.extend(scope.extras.iter().filter(|extra| extra.is_file()).cloned());
    paths.sort();
    paths.dedup();
    if paths.len() > MAX_DOCUMENTS {
        return Err(ScanError::TooMany {
            root: root.to_path_buf(),
            count: paths.len(),
        });
    }
    let mut documents = paths
        .iter()
        .map(|path| read_document(root, path, kind))
        .collect::<Result<Vec<_>, _>>()?;
    if kind == SourceKind::Instructions {
        documents.retain(|document| !is_generated(&document.body));
    }
    documents.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(documents)
}

fn is_generated(body: &str) -> bool {
    body.trim_start().starts_with(GENERATED_MARKER)
}

fn included_paths(root: &Path, includes: &[String]) -> Result<Vec<PathBuf>, ScanError> {
    let mut found = Vec::new();
    for name in includes {
        let path = root.join(name);
        match fs::metadata(&path) {
            Ok(meta) if meta.is_dir() => found.extend(tree_paths(&path, &[])?),
            Ok(meta) if meta.is_file() => found.push(path),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(ScanError::Io { path, source }),
        }
    }
    Ok(found)
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
            } else if file_type.is_file() && is_markdown(&path) && !is_thread_worklog(&path) {
                found.push(path);
            }
        }
    }
    Ok(found)
}

fn is_thread_worklog(path: &Path) -> bool {
    let named =
        |part: Option<&std::ffi::OsStr>, name: &str| part.and_then(|p| p.to_str()) == Some(name);
    named(path.file_name(), WORKLOG_FILE)
        && named(
            path.parent()
                .and_then(Path::parent)
                .and_then(Path::file_name),
            THREADS_DIRECTORY,
        )
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
    let inside = match path.strip_prefix(root) {
        Ok(inner) => inner,
        Err(_) => path.file_name().map_or(path, Path::new),
    };
    inside
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
        SourceKind::Memory | SourceKind::Skills | SourceKind::Instructions => {
            text_field(metadata, "name")
        }
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

pub fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
