use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::{MachineIdentity, MachineIdentityError, file_in, read_file};
use crate::store::persistence::machine_merge::MachineMergeReport;
use crate::store::persistence::{PersistError, PersistHandle};

const LEGACY_DIR: &str = "mike-knowledge";
const MERGED_FILE: &str = "machine.json.merged";

#[derive(Debug, thiserror::Error)]
pub enum MergeError {
    #[error(transparent)]
    Identity(#[from] MachineIdentityError),
    #[error("cannot merge machine identity {legacy} into {current}: {source}")]
    Persist {
        legacy: Uuid,
        current: Uuid,
        #[source]
        source: PersistError,
    },
    #[error("merged machine identity {legacy} but cannot rename {path}: {source}")]
    Rename {
        legacy: Uuid,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl MergeError {
    fn is_retryable(&self) -> bool {
        matches!(self, Self::Persist { .. })
    }
}

pub fn legacy_state_dir(home: Option<&str>) -> Option<PathBuf> {
    home.filter(|value| !value.is_empty()).map(|home| {
        PathBuf::from(home)
            .join(".local")
            .join("state")
            .join(LEGACY_DIR)
    })
}

pub async fn merge_legacy_identity(
    persist: &PersistHandle,
    identity: &MachineIdentity,
    legacy_dir: &Path,
) -> Result<Option<(Uuid, MachineMergeReport)>, MergeError> {
    let path = file_in(legacy_dir);
    let Some(legacy) = read_file(&path).await? else {
        return Ok(None);
    };
    if legacy.machine_uuid == identity.machine_uuid {
        return Ok(None);
    }
    let report = persist
        .merge_machine_identity(
            legacy.machine_uuid,
            identity.machine_uuid,
            &identity.machine_name,
        )
        .await
        .map_err(|source| MergeError::Persist {
            legacy: legacy.machine_uuid,
            current: identity.machine_uuid,
            source,
        })?;
    tokio::fs::rename(&path, legacy_dir.join(MERGED_FILE))
        .await
        .map_err(|source| MergeError::Rename {
            legacy: legacy.machine_uuid,
            path: path.clone(),
            source,
        })?;
    Ok(Some((legacy.machine_uuid, report)))
}

pub async fn merge_legacy_at_startup(
    persist: &PersistHandle,
    identity: &MachineIdentity,
) -> bool {
    let Some(dir) = legacy_state_dir(std::env::var("HOME").ok().as_deref()) else {
        return true;
    };
    match merge_legacy_identity(persist, identity, &dir).await {
        Ok(Some((legacy, report))) => {
            log_merge(legacy, identity.machine_uuid, &report);
            true
        }
        Ok(None) => true,
        Err(error) => {
            tracing::warn!(%error, retry = error.is_retryable(), "machine_identity: legacy identity merge failed");
            !error.is_retryable()
        }
    }
}

fn log_merge(legacy: Uuid, machine: Uuid, report: &MachineMergeReport) {
    tracing::info!(
        legacy_machine = %legacy,
        machine = %machine,
        locations_moved = report.locations_moved,
        locations_merged = report.locations_merged,
        sessions_repointed = report.sessions_repointed,
        sources_merged = report.sources_merged,
        documents_merged = report.documents_merged,
        llm_calls_updated = report.llm_calls_updated,
        worker_leases_updated = report.worker_leases_updated,
        "machine_identity: merged the legacy mike-knowledge identity into this machine's identity"
    );
}

#[cfg(test)]
#[path = "merge_db_tests.rs"]
pub mod db_tests;
