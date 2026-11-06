use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

use serde_json::{Value, json};
use uuid::Uuid;

use super::profile_for;
use crate::launch::{LaunchError, SplitLaunchArgs};

pub(crate) const MANIFEST_ENV: &str = "MIKE_ACCEPTANCE_MANIFEST";
const VERSION: u64 = 1;
const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;

pub(crate) fn for_launch(
    briefs: &Path,
    split: &SplitLaunchArgs,
    session: Option<Uuid>,
) -> Result<Option<PathBuf>, LaunchError> {
    let Some(run) = split.run.as_deref().filter(|run| run.starts_with("fleet-")) else {
        return Ok(None);
    };
    let kind = split.role.as_deref().or(split.seat.as_deref());
    if kind.is_none_or(|kind| profile_for(kind) != "acceptance") {
        return Ok(None);
    }
    prepare(briefs, run, session).map(Some)
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

fn initial(run: &str, session: Option<Uuid>) -> Value {
    json!({
        "version": VERSION,
        "run_id": run,
        "created_by_session": session,
        "created_at": chrono::Utc::now(),
        "obligation_register": {"source": "mike session notes", "tag": "acceptance-obligation"},
        "comparisons": [],
        "artifacts": [],
        "close_handoff": null,
    })
}

fn publish(path: &Path, run: &str, session: Option<Uuid>) -> Result<(), LaunchError> {
    let parent = path.parent().ok_or_else(|| {
        LaunchError::Refused("acceptance manifest has no containing directory".into())
    })?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| LaunchError::io("staging acceptance manifest", error))?;
    serde_json::to_writer_pretty(&mut staged, &initial(run, session))
        .map_err(|error| LaunchError::Failed(format!("writing acceptance manifest: {error}")))?;
    staged
        .flush()
        .and_then(|()| staged.as_file().sync_all())
        .map_err(|error| LaunchError::io("saving acceptance manifest", error))?;
    match staged.persist_noclobber(path) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(LaunchError::io(
            "publishing acceptance manifest",
            error.error,
        )),
    }
}

fn read_manifest(path: &Path) -> Result<File, LaunchError> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| LaunchError::io("opening acceptance manifest", error))?;
    let metadata = file
        .metadata()
        .map_err(|error| LaunchError::io("checking acceptance manifest", error))?;
    if !metadata.is_file() {
        return Err(LaunchError::Refused(
            "acceptance manifest is not a regular file".into(),
        ));
    }
    Ok(file)
}

fn verify(path: &Path, run: &str) -> Result<(), LaunchError> {
    let mut bytes = Vec::new();
    read_manifest(path)?
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| LaunchError::io("reading acceptance manifest", error))?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(LaunchError::Refused(format!(
            "acceptance manifest exceeds {MAX_MANIFEST_BYTES} bytes; keep payloads in referenced files"
        )));
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| LaunchError::Failed(format!("reading acceptance manifest: {error}")))?;
    if value["version"].as_u64() != Some(VERSION)
        || value["run_id"].as_str() != Some(run)
        || !value["artifacts"].is_array()
        || !value["comparisons"].is_array()
    {
        return Err(LaunchError::Refused(format!(
            "acceptance manifest has an incompatible schema or run identity: {}",
            path.display()
        )));
    }
    Ok(())
}

fn prepare(briefs: &Path, run: &str, session: Option<Uuid>) -> Result<PathBuf, LaunchError> {
    run_component(run)?;
    let briefs = trusted_root(briefs)?;
    let root = briefs.join("fleet-runs").join(run).join("acceptance");
    directory(&root)?;
    for child in ["comparisons", "artifacts"] {
        directory(&root.join(child))?;
    }
    let manifest = root.join("manifest.json");
    publish(&manifest, run, session)?;
    verify(&manifest, run)?;
    Ok(manifest)
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

pub(crate) fn brief_reference(path: &Path) -> String {
    format!(
        "Acceptance working record: {} ({MANIFEST_ENV}). Reuse this run's manifest and files across successors. Keep live comparison scratch and artifact provenance there; retrieve only the entries needed. Existing obligation notes remain authoritative: seed missing obligations only, supersede only changed ones. At rotation checkpoint the same files; at run close publish one handoff referencing the manifest and note ids. Missing artifacts or observations remain unknown, never proof. Keep the existing context safety limit.",
        path.display()
    )
}

#[cfg(test)]
#[path = "acceptance_artifacts_tests.rs"]
mod tests;
