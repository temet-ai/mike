use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::error::ClaudeCodeError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Edited {
    Unchanged,
    Written,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Removal {
    Removed,
    NotThere,
    NotOurs,
}

fn write_error(path: &Path, source: std::io::Error) -> ClaudeCodeError {
    ClaudeCodeError::Write {
        path: path.to_path_buf(),
        source,
    }
}

fn resolve(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(unix)]
fn existing_mode(path: &Path, default: u32) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).map_or(default, |m| m.permissions().mode() & 0o777)
}

#[cfg(unix)]
fn apply_mode(file: &std::fs::File, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(std::fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn existing_mode(_: &Path, default: u32) -> u32 {
    default
}

#[cfg(not(unix))]
fn apply_mode(_: &std::fs::File, _: u32) -> std::io::Result<()> {
    Ok(())
}

pub(crate) fn write_text(
    path: &Path,
    text: &str,
    default_mode: u32,
) -> Result<(), ClaudeCodeError> {
    let target = resolve(path);
    let dir = target
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| write_error(&target, e))?;
    let mode = existing_mode(&target, default_mode);
    let mut staged = tempfile::Builder::new()
        .prefix(".mike-staged-")
        .tempfile_in(dir)
        .map_err(|e| write_error(&target, e))?;
    staged
        .write_all(text.as_bytes())
        .map_err(|e| write_error(&target, e))?;
    apply_mode(staged.as_file(), mode).map_err(|e| write_error(&target, e))?;
    staged
        .as_file()
        .sync_all()
        .map_err(|e| write_error(&target, e))?;
    staged
        .persist(&target)
        .map_err(|e| write_error(&target, e.error))?;
    Ok(())
}

pub(crate) fn read_text(path: &Path) -> Result<Option<String>, ClaudeCodeError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ClaudeCodeError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

pub(crate) fn read_object(path: &Path) -> Result<Map<String, Value>, ClaudeCodeError> {
    let Some(text) = read_text(path)? else {
        return Ok(Map::new());
    };
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(other) => Err(ClaudeCodeError::Shape {
            path: path.to_path_buf(),
            found: json_kind(&other).to_string(),
            expected: "a JSON object",
        }),
        Err(e) => Err(ClaudeCodeError::Parse {
            path: path.to_path_buf(),
            message: e.to_string(),
        }),
    }
}

pub(crate) fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}

pub(crate) fn edit_object<T>(
    path: &Path,
    change: impl FnOnce(&mut Map<String, Value>) -> Result<T, ClaudeCodeError>,
) -> Result<(Edited, T), ClaudeCodeError> {
    let mut root = read_object(path)?;
    let before = root.clone();
    let value = change(&mut root)?;
    if root == before {
        return Ok((Edited::Unchanged, value));
    }
    let text = serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|e| ClaudeCodeError::Other(format!("cannot encode {}: {e}", path.display())))?;
    write_text(path, &format!("{text}\n"), 0o600)?;
    Ok((Edited::Written, value))
}

pub(crate) fn object_member<'a>(
    root: &'a mut Map<String, Value>,
    key: &str,
    path: &Path,
) -> Result<&'a mut Map<String, Value>, ClaudeCodeError> {
    let member = root
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let found = json_kind(member);
    member
        .as_object_mut()
        .ok_or_else(|| ClaudeCodeError::Shape {
            path: path.to_path_buf(),
            found: format!("{found} for `{key}`"),
            expected: "an object there",
        })
}
