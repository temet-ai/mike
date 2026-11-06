use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub(super) fn resolve(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return validate(path);
    }
    let current = std::env::current_exe().context("cannot locate the running mike binary")?;
    let invocation = std::env::args_os().next().map(PathBuf::from);
    let path = std::env::var_os("PATH");
    let candidates = candidates(&current, invocation.as_deref(), path.as_deref());
    candidates
        .iter()
        .find_map(|path| validate(path).ok())
        .context("matching MCP companion not found; install mike-mcp beside mike or provide --binary with a stable absolute path")
}

pub(super) fn candidates(
    current: &Path,
    invocation: Option<&Path>,
    search_path: Option<&std::ffi::OsStr>,
) -> Vec<PathBuf> {
    let mut candidates = stable_prefixes(current);
    if let Some(invocation) = invocation
        && invocation.components().count() > 1
        && std::fs::canonicalize(invocation).ok() == std::fs::canonicalize(current).ok()
        && let Some(parent) = invocation.parent()
    {
        candidates.push(parent.join("mike-mcp"));
    }
    if let Some(parent) = current.parent() {
        candidates.push(parent.join("mike-mcp"));
    }
    if let Some(search_path) = search_path {
        candidates.extend(std::env::split_paths(search_path).map(|path| path.join("mike-mcp")));
    }
    candidates
}

fn stable_prefixes(current: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for ancestor in current.ancestors() {
        match ancestor.file_name().and_then(|name| name.to_str()) {
            Some("mike-archive") => {
                if let Some(prefix) = ancestor.parent().and_then(Path::parent) {
                    candidates.push(prefix.join("bin/mike-mcp"));
                }
            }
            Some("Cellar") => {
                if let Some(prefix) = ancestor.parent() {
                    candidates.push(prefix.join("bin/mike-mcp"));
                    candidates.push(prefix.join("opt/mike-mcp/bin/mike-mcp"));
                }
            }
            _ => {}
        }
    }
    candidates
}

pub(super) fn validate(path: &Path) -> Result<PathBuf> {
    let path = super::absolute(path)?;
    anyhow::ensure!(
        !path
            .components()
            .any(|component| component.as_os_str() == "Cellar")
            && !path.ancestors().any(|ancestor| {
                ancestor
                    .file_name()
                    .is_some_and(|name| name == "generations")
                    && ancestor
                        .parent()
                        .and_then(Path::file_name)
                        .is_some_and(|name| name == "mike-archive")
            }),
        "version-specific MCP path refused; use the installation prefix's bin/mike-mcp or Homebrew opt/mike-mcp/bin/mike-mcp"
    );
    let metadata = std::fs::metadata(&path)
        .with_context(|| format!("MCP companion is missing: {}", path.display()))?;
    anyhow::ensure!(
        metadata.is_file(),
        "MCP companion is not a regular executable"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        anyhow::ensure!(
            metadata.permissions().mode() & 0o111 != 0,
            "MCP companion is not executable"
        );
    }
    Ok(path)
}
