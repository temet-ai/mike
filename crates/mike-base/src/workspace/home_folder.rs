use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::store::persistence::project_identity::ProjectIdentity;
use crate::store::persistence::{PersistError, PersistHandle};

const WORKSPACE_MARKERS: [&str; 3] = ["CLAUDE.md", "AGENTS.md", ".claude"];
const TRANSIENT_SEGMENTS: [&str; 3] = ["/.cache/", "/.local/state/", "/warm/cwd"];

pub fn is_transient(cwd: &Path) -> bool {
    let text = format!("{}/", cwd.to_string_lossy().trim_end_matches('/'));
    TRANSIENT_SEGMENTS.iter().any(|segment| text.contains(segment))
}

pub fn workspace_folder(cwd: &Path, home: &Path, claude_home: &Path) -> Option<PathBuf> {
    if is_transient(cwd) {
        return None;
    }
    cwd.ancestors()
        .take_while(|dir| *dir != home && dir.starts_with(home))
        .filter(|dir| *dir != claude_home)
        .find(|dir| WORKSPACE_MARKERS.iter().any(|marker| dir.join(marker).exists()))
        .map(Path::to_path_buf)
}

pub fn top_folder(cwd: &Path, home: &Path) -> Option<PathBuf> {
    let first = cwd.strip_prefix(home).ok()?.components().next()?;
    Some(home.join(first))
}

pub fn host_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute() && home != Path::new("/"))
}

pub fn claude_home(home: &Path) -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"))
}

pub async fn bind_in_machine_home(
    persist: &PersistHandle,
    session: Uuid,
    machine: Uuid,
    cwd: &str,
) -> Result<Option<ProjectIdentity>, PersistError> {
    let Some(home) = host_home() else {
        return Ok(None);
    };
    let cwd = Path::new(cwd);
    if !cwd.starts_with(&home) || is_transient(cwd) {
        return Ok(None);
    }
    if let Some(top) = top_folder(cwd, &home)
        && persist
            .folder_holds_other_projects(machine, &top.to_string_lossy())
            .await?
    {
        return Ok(None);
    }
    let root = workspace_folder(cwd, &home, &claude_home(&home)).unwrap_or(home);
    let Some(location) = persist
        .ensure_machine_home_location(machine, &root.to_string_lossy())
        .await?
    else {
        return Ok(None);
    };
    persist.bind_session_location(session, location).await.map(Some)
}

#[cfg(test)]
#[path = "home_folder_tests.rs"]
mod tests;
