use std::io::ErrorKind;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Component, Path, PathBuf};

use crate::launch::seat::profile_for;
use crate::launch::{LaunchError, SplitLaunchArgs};

pub const EVIDENCE_ENV: &str = "MIKE_ACCEPTANCE_EVIDENCE";

pub fn for_launch(briefs: &Path, split: &SplitLaunchArgs) -> Result<Option<PathBuf>, LaunchError> {
    let Some(run) = split.run.as_deref().filter(|run| run.starts_with("fleet-")) else {
        return Ok(None);
    };
    let kind = split.role.as_deref().or(split.seat.as_deref());
    if kind.is_none_or(|kind| profile_for(kind) != "acceptance") {
        return Ok(None);
    }
    prepare(briefs, run).map(Some)
}

fn run_component(run: &str) -> Result<(), LaunchError> {
    if !run.starts_with("fleet-")
        || run.len() <= "fleet-".len()
        || !run
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(LaunchError::Refused(
            "invalid acceptance artifact run id".into(),
        ));
    }
    Ok(())
}

fn directory(path: &Path) -> Result<(), LaunchError> {
    if !path.is_absolute() {
        return Err(LaunchError::Refused(
            "acceptance artifact storage must be absolute".into(),
        ));
    }
    let mut checked = PathBuf::new();
    for component in path.components() {
        if !matches!(component, Component::RootDir | Component::Normal(_)) {
            return Err(LaunchError::Refused(
                "invalid acceptance artifact storage path".into(),
            ));
        }
        checked.push(component);
        ensure_directory(&checked)?;
    }
    Ok(())
}

fn ensure_directory(path: &Path) -> Result<(), LaunchError> {
    match std::fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(LaunchError::io(
                "creating acceptance artifact directory",
                error,
            ));
        }
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| LaunchError::io("checking acceptance artifact directory", error))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(LaunchError::Refused(format!(
            "acceptance artifact directory is not a real directory: {}",
            path.display()
        )));
    }
    Ok(())
}

fn prepare(briefs: &Path, run: &str) -> Result<PathBuf, LaunchError> {
    run_component(run)?;
    let briefs = trusted_root(briefs)?;
    let root = briefs.join("fleet-runs").join(run).join("acceptance");
    directory(&root)?;
    for child in ["comparisons", "artifacts"] {
        directory(&root.join(child))?;
    }
    Ok(root)
}

fn trusted_root(briefs: &Path) -> Result<PathBuf, LaunchError> {
    if !briefs.is_absolute() || briefs.components().any(|part| part == Component::ParentDir) {
        return Err(LaunchError::Refused(
            "acceptance artifact storage must be absolute without parent traversal".into(),
        ));
    }
    std::fs::create_dir_all(briefs)
        .map_err(|error| LaunchError::io("creating configured brief storage", error))?;
    std::fs::canonicalize(briefs)
        .map_err(|error| LaunchError::io("resolving configured brief storage", error))
}

pub fn brief_reference(run: &str, folder: &Path) -> String {
    format!(
        "Acceptance record for run {run}: it lives in mike session notes, not in files. \
Record each obligation, comparison, artifact and the close handoff as a note on your own session \
with write_session_note, tagged {run} and acceptance-record (obligations keep their \
acceptance-obligation tag). An artifact note gives the absolute path of any file, commit SHA, \
features, environment, outcome and cleanup status. A successor reads the record with \
list_session_notes project_path=<this repository> tag={run}, which returns this run's \
record and no other run's; obligations are read as before. Existing notes stay authoritative: add \
only missing ones, supersede only changed ones. Put only binary evidence, such as screenshots and \
logs, in {folder} ({EVIDENCE_ENV}), under comparisons/ and artifacts/, and name each file's \
absolute path in its note. Missing notes or files remain unknown, never proof. Keep the \
existing context safety limit.",
        folder = folder.display()
    )
}

#[cfg(test)]
#[path = "acceptance_artifacts_tests.rs"]
mod tests;
