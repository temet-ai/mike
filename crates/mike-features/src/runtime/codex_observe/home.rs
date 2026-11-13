use crate::store::persistence::runtime_repo::RuntimePersist as _;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::runtime::{ProviderThreadRef, RuntimeKind, RuntimeNamespace};
use crate::store::persistence::{PersistError, PersistHandle};

pub async fn resolve_bound_session(
    persist: &PersistHandle,
    home: &Path,
    thread: &str,
) -> Result<Option<Uuid>, PersistError> {
    let Some(namespace) = verified_namespace(persist, home).await? else {
        return Ok(None);
    };
    Ok(persist
        .runtime()
        .runtime_thread(&ProviderThreadRef {
            namespace_id: namespace.id,
            thread_id: thread.to_owned(),
        })
        .await?
        .map(|binding| binding.mike_session_id))
}

pub async fn verified_namespace(
    persist: &PersistHandle,
    home: &Path,
) -> Result<Option<RuntimeNamespace>, PersistError> {
    let canonical = match tokio::fs::canonicalize(home).await {
        Ok(home) => home,
        Err(_) => return Ok(None),
    };
    for candidate in [home, canonical.as_path()] {
        let expected = crate::runtime::codex_home::namespace_for(candidate);
        if let Some(stored) = persist.runtime().runtime_namespace(expected.id).await?
            && stored.runtime == RuntimeKind::Codex
            && stored.durable_key == expected.durable_key
        {
            return Ok(Some(stored));
        }
    }
    let expected = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")));
    let Some(expected) = expected else {
        return Ok(None);
    };
    if tokio::fs::canonicalize(expected).await.ok().as_ref() == Some(&canonical) {
        return Ok(Some(crate::runtime::codex_home::namespace_for(&canonical)));
    }
    Ok(None)
}
