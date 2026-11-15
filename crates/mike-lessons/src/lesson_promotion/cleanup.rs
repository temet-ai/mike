use std::path::Path;

use serde::Serialize;

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub(super) struct CleanupReport {
    pub folder: Option<String>,
    pub folder_removed: bool,
    pub error: Option<String>,
}

pub(super) fn clean_up(data_root: &Path, apply: bool) -> CleanupReport {
    let mut report = CleanupReport::default();
    let folder = data_root.join("artifacts/learning-memory");
    if !folder.is_dir() {
        return report;
    }
    report.folder = Some(folder.display().to_string());
    if apply {
        match std::fs::remove_dir_all(&folder) {
            Ok(()) => report.folder_removed = true,
            Err(error) => report.error = Some(format!("{}: {error}", folder.display())),
        }
    }
    report
}
