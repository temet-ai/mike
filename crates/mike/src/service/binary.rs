use std::path::{Component, Path, PathBuf};

use super::error::ServiceError;

pub(crate) fn stable_path(current: &Path) -> Result<PathBuf, ServiceError> {
    let resolved = std::fs::canonicalize(current).map_err(ServiceError::Binary)?;
    Ok(prefer_homebrew_link(&resolved))
}

fn prefer_homebrew_link(resolved: &Path) -> PathBuf {
    match homebrew_link(resolved) {
        Some(link) if same_file(&link, resolved) => link,
        _ => resolved.to_path_buf(),
    }
}

pub(crate) fn homebrew_link(resolved: &Path) -> Option<PathBuf> {
    let parts: Vec<Component<'_>> = resolved.components().collect();
    let cellar = parts
        .iter()
        .position(|part| matches!(part, Component::Normal(name) if *name == "Cellar"))?;
    let [formula, version, bin, name] = parts.get(cellar + 1..)? else {
        return None;
    };
    let is_bin = matches!(bin, Component::Normal(dir) if *dir == "bin");
    let named = |c: &Component<'_>| matches!(c, Component::Normal(_));
    if !is_bin || ![formula, version, name].into_iter().all(named) {
        return None;
    }
    let prefix: PathBuf = parts.get(..cellar)?.iter().collect();
    Some(prefix.join("bin").join(name.as_os_str()))
}

fn same_file(link: &Path, resolved: &Path) -> bool {
    std::fs::canonicalize(link).is_ok_and(|target| target == resolved)
}
