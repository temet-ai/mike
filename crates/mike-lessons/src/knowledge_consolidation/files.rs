use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum EditError {
    Mismatch(PathBuf),
    Io(PathBuf, String),
}

impl std::error::Error for EditError {}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mismatch(path) => write!(
                f,
                "{} no longer holds the passage at its recorded place; the file was not touched",
                path.display()
            ),
            Self::Io(path, error) => write!(f, "{}: {error}", path.display()),
        }
    }
}

fn is_list_marker(prefix: &str) -> bool {
    let trimmed = prefix.trim();
    if trimmed.is_empty() {
        return true;
    }
    if matches!(trimmed, "-" | "*" | "+") {
        return true;
    }
    trimmed
        .strip_suffix('.')
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

fn expand_to_line(bytes: &[u8], start: usize, end: usize) -> (usize, usize) {
    let line_start = bytes[..start]
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(0, |i| i + 1);
    let line_end = bytes[end..]
        .iter()
        .position(|b| *b == b'\n')
        .map_or(bytes.len(), |i| end + i);
    let before = std::str::from_utf8(&bytes[line_start..start]).unwrap_or("x");
    let after = std::str::from_utf8(&bytes[end..line_end]).unwrap_or("x");
    if !is_list_marker(before) || !after.trim().is_empty() {
        return (start, end);
    }
    let removed_end = if line_end < bytes.len() {
        line_end + 1
    } else {
        line_end
    };
    (line_start, removed_end)
}

const CONTEXT_BYTES: usize = 48;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditRecord {
    pub path: String,
    pub at: usize,
    pub removed: String,
    #[serde(default)]
    pub inserted: String,
    pub before: String,
    pub after: String,
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn edit_record(path: &Path, bytes: &[u8], from: usize, to: usize) -> EditRecord {
    EditRecord {
        path: path.to_string_lossy().into_owned(),
        at: from,
        removed: lossy(&bytes[from..to]),
        inserted: String::new(),
        before: lossy(&bytes[from.saturating_sub(CONTEXT_BYTES)..from]),
        after: lossy(&bytes[to..(to + CONTEXT_BYTES).min(bytes.len())]),
    }
}

pub fn removal_span(bytes: &[u8], start: usize, end: usize, passage: &str) -> Option<(usize, usize)> {
    if bytes.get(start..end)? != passage.as_bytes() {
        return None;
    }
    Some(expand_to_line(bytes, start, end))
}

pub fn reinsert_into(bytes: &[u8], record: &EditRecord) -> Option<Vec<u8>> {
    let at = record.at;
    let before_start = at.checked_sub(record.before.len())?;
    let inserted_end = at.checked_add(record.inserted.len())?;
    let after_end = inserted_end.checked_add(record.after.len())?;
    if after_end > bytes.len()
        || bytes[before_start..at] != *record.before.as_bytes()
        || bytes[at..inserted_end] != *record.inserted.as_bytes()
        || bytes[inserted_end..after_end] != *record.after.as_bytes()
    {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() + record.removed.len());
    out.extend_from_slice(&bytes[..at]);
    out.extend_from_slice(record.removed.as_bytes());
    out.extend_from_slice(&bytes[inserted_end..]);
    Some(out)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    if let Ok(meta) = std::fs::metadata(path) {
        std::fs::set_permissions(temp.path(), meta.permissions())?;
    }
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

pub fn remove_passage(
    path: &Path,
    start: usize,
    end: usize,
    passage: &str,
) -> Result<EditRecord, EditError> {
    let io = |e: std::io::Error| EditError::Io(path.to_path_buf(), e.to_string());
    let bytes = std::fs::read(path).map_err(io)?;
    let (from, to) = removal_span(&bytes, start, end, passage)
        .ok_or_else(|| EditError::Mismatch(path.to_path_buf()))?;
    let record = edit_record(path, &bytes, from, to);
    let mut edited = Vec::with_capacity(bytes.len() - (to - from));
    edited.extend_from_slice(&bytes[..from]);
    edited.extend_from_slice(&bytes[to..]);
    write_atomic(path, &edited).map_err(io)?;
    Ok(record)
}

pub fn reinsert(record: &EditRecord) -> Result<(), EditError> {
    let path = Path::new(&record.path);
    let io = |e: std::io::Error| EditError::Io(path.to_path_buf(), e.to_string());
    let bytes = std::fs::read(path).map_err(io)?;
    let restored = reinsert_into(&bytes, record).ok_or_else(|| EditError::Mismatch(path.to_path_buf()))?;
    write_atomic(path, &restored).map_err(io)
}

pub fn replace_passage(
    path: &Path,
    (start, end): (usize, usize),
    passage: &str,
    text: &str,
) -> Result<EditRecord, EditError> {
    let io = |e: std::io::Error| EditError::Io(path.to_path_buf(), e.to_string());
    let bytes = std::fs::read(path).map_err(io)?;
    if bytes.get(start..end) != Some(passage.as_bytes()) {
        return Err(EditError::Mismatch(path.to_path_buf()));
    }
    let mut record = edit_record(path, &bytes, start, end);
    record.inserted = text.to_owned();
    let mut edited = Vec::with_capacity(bytes.len() + text.len());
    edited.extend_from_slice(&bytes[..start]);
    edited.extend_from_slice(text.as_bytes());
    edited.extend_from_slice(&bytes[end..]);
    write_atomic(path, &edited).map_err(io)?;
    Ok(record)
}
